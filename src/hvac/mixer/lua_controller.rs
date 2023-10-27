use std::{
    collections::{BTreeMap, BTreeSet},
    future::IntoFuture,
    sync::Arc,
};

use chrono::NaiveTime;
use mlua::prelude::*;
use models::hvac_request::HvacRequest;
use redis::AsyncCommands;
use tokio::{runtime::Runtime, sync::Mutex, task::LocalSet};

use crate::{
    hvac::{probe::Probe, Probes},
    mqtt::MqttClient,
    RedisConn,
};

use super::MixerState;

#[derive(Clone)]
pub struct LuaController {
    state: Arc<Mutex<LuaControllerState>>,
    task_tx: tokio::sync::mpsc::Sender<LuaExecTask>,
}

impl Default for LuaController {
    fn default() -> LuaController {
        let task_tx = create_lua_thread();
        LuaController {
            state: Default::default(),
            task_tx,
        }
    }
}

type LuaExecTask = Box<dyn FnOnce(&LocalSet) + Send + 'static>;

fn create_lua_thread() -> tokio::sync::mpsc::Sender<LuaExecTask> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<LuaExecTask>(16);

    std::thread::spawn(move || {
        let rt = Runtime::new().unwrap();
        let localset = LocalSet::new();
        localset.block_on(&rt, async {
            while let Some(task) = rx.recv().await {
                task(&localset);
            }
        })
    });

    tx
}

impl LuaController {
    async fn exec_lua_thread<Fn, Fu, R>(&self, f: Fn) -> anyhow::Result<R>
    where
        Fn: FnOnce() -> Fu + Send + 'static,
        Fu: IntoFuture<Output = R> + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.task_tx
            .send(Box::new(move |localset| {
                localset.spawn_local(async move {
                    tx.send(f().await).ok();
                });
            }))
            .await
            .ok();
        Ok(rx.await?)
    }

    fn fire_lua_thread<Fn, Fu>(&self, f: Fn) -> anyhow::Result<()>
    where
        Fn: FnOnce() -> Fu + Send + 'static,
        Fu: IntoFuture<Output = ()> + 'static,
    {
        self.task_tx
            .blocking_send(Box::new(move |localset| {
                localset.spawn_local(async move {
                    f().await;
                });
            }))
            .map_err(|e| anyhow::anyhow!("fire_lua_thread: {e}"))?;
        Ok(())
    }

    pub async fn is_loaded(&self) -> bool {
        let state = self.state.lock().await;
        state.is_loaded()
    }

    pub async fn validate(
        &self,
        script: String,
        mixer: MixerState,
    ) -> anyhow::Result<(Option<HvacRequest>, BTreeSet<String>)> {
        // Lock the state so the issues won't fill up
        let mut _state = self.state.lock().await;

        let result = self
            .exec_lua_thread(move || async move {
                reset_issues();
                let mut temp_state = LuaControllerState::default();
                temp_state.load(&script, mixer.clone()).await?;
                let result = temp_state.evaluate(mixer).await?;
                Ok::<_, anyhow::Error>(result)
            })
            .await??;

        Ok((result, issues()))
    }

    pub async fn load(&self, script: String, mixer: MixerState) -> anyhow::Result<()> {
        let state = self.state.clone();
        self.exec_lua_thread(move || async move {
            let mut state = state.lock().await;
            state.load(&script, mixer).await
        })
        .await?
    }

    pub async fn evaluate(&self, mixer: MixerState) -> anyhow::Result<Option<HvacRequest>> {
        let state = self.state.clone();
        self.exec_lua_thread(move || async move {
            let mut state = state.lock().await;
            state.evaluate(mixer).await
        })
        .await?
    }

    pub async fn load_redis(&self, redis: &RedisConn, mixer: MixerState) -> anyhow::Result<()> {
        let script = {
            let mut redis = redis.get();
            redis.get("thermostat.lua.current").await?
        };

        self.load(script, mixer).await
    }

    pub fn on_mqtt(&self, mixer: MixerState, topic: &str, payload: &[u8]) {
        let topic = topic.to_string();
        let payload = String::from_utf8_lossy(payload).to_string();
        let state = self.state.clone();
        self.fire_lua_thread(move || async move {
            let mut state = state.lock().await;
            state.on_mqtt(mixer, topic, payload).await;
        })
        .ok();
    }

    pub async fn tick(&self, mixer: MixerState) {
        let state = self.state.clone();
        self.exec_lua_thread(move || async move {
            let mut state = state.lock().await;
            state.tick(mixer).await;
        })
        .await
        .ok();
    }
}

struct LuaControllerState {
    lua: Lua,
}

impl Default for LuaControllerState {
    fn default() -> Self {
        LuaControllerState { lua: Lua::new() }
    }
}

impl LuaControllerState {
    fn is_loaded(&self) -> bool {
        self.lua.globals().get::<_, LuaFunction>("evaluate").is_ok()
    }

    async fn load(&mut self, script: &str, mixer: MixerState) -> anyhow::Result<()> {
        self.lua.load(script).exec_async().await?;

        if let Ok(init) = self.lua.globals().get::<_, LuaFunction>("init") {
            let () = init.call_async(mixer).await?;
        }

        Ok(())
    }

    async fn evaluate(&mut self, mixer: MixerState) -> anyhow::Result<Option<HvacRequest>> {
        let evaluate: LuaFunction = self.lua.globals().get("evaluate")?;
        let result: Option<String> = evaluate.call_async(mixer).await?;

        Ok(result.and_then(|res| HvacRequest::from_payload(res.as_bytes())))
    }

    async fn on_mqtt(&mut self, mixer: MixerState, topic: String, payload: String) {
        let Ok(on_mqtt) = self.lua.globals().get::<_, LuaFunction>("onmqtt") else {
            return;
        };

        let _: Result<(), _> = on_mqtt.call_async((mixer, topic, payload)).await;
    }

    async fn tick(&mut self, mixer: MixerState) {
        let Ok(tick) = self.lua.globals().get::<_, LuaFunction>("tick") else {
            return;
        };

        let _: Result<(), _> = tick.call_async(mixer).await;
    }
}

impl LuaUserData for MixerState {
    fn add_fields<'lua, F: LuaUserDataFields<'lua, Self>>(fields: &mut F) {
        fields.add_field_method_get("redis", |_, this| Ok(this.redis.clone()));
        fields.add_field_method_get("probes", |_, this| Ok(this.probes.clone()));
        fields.add_field_method_get("mode", |_, this| Ok(this.mode().payload_str()));
        fields.add_field_method_get("last_result", |_, this| {
            Ok(this.last_result.load().payload_str())
        });
    }

    fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_async_method("timed_program", |lua, _this, program: LuaTable| async move {
            let mut program_table = BTreeMap::new();
            for pair in program.pairs::<String, LuaFunction>() {
                let Ok((time_str, func)) = pair else {
                    add_issue(format!(
                        "[src:{}] Timed program must be passed a table mapping time strings to functions",
                        lua.inspect_stack(1).map(|d| d.curr_line()).unwrap_or(-1)
                    ));
                    continue
                };
                let Ok(time) = NaiveTime::parse_from_str(&time_str, "%H:%M") else {
                    add_issue(format!(
                        "[src:{}] Timed program keys must be in 'HH:MM' format. Found {time_str}",
                        lua.inspect_stack(1).map(|d| d.curr_line()).unwrap_or(-1)
                    ));
                    continue
                };
                program_table.insert(time, func);
            }
            if program_table.is_empty() {
                add_issue(format!(
                    "[src:{}] Empty program table",
                    lua.inspect_stack(1).map(|d| d.curr_line()).unwrap_or(-1)
                ));
                return Ok(LuaValue::Nil)
            }

            let current_time = chrono::Local::now().time();
            let mut active_time = *program_table.last_key_value().unwrap().0;
            for time in program_table.keys() {
                if *time < current_time {
                    active_time = *time;
                } else {
                    break;
                }
            }

            program_table[&active_time].call_async(()).await
        });
    }
}

impl LuaUserData for Probes {
    fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_async_meta_method(LuaMetaMethod::Index, |_, this, probe: String| async move {
            Ok(this.get(&probe).await)
        });
    }
}

impl LuaUserData for Probe {
    fn add_fields<'lua, F: LuaUserDataFields<'lua, Self>>(fields: &mut F) {
        fields.add_field_method_get("temperature", |_, this| Ok(this.value()))
    }
}

impl LuaUserData for RedisConn {
    fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_async_method("get", |_, redis, key: String| async move {
            let mut redis = redis.get();
            let result: String = redis
                .get(key)
                .await
                .map_err(|e| LuaError::ExternalError(Arc::new(e)))?;
            Ok(result)
        });
        methods.add_async_method("set", |_, redis, args: (String, String)| async move {
            let (key, value) = args;
            let mut redis = redis.get();
            let result: bool = redis
                .set(key, value)
                .await
                .map_err(|e| LuaError::ExternalError(Arc::new(e)))?;
            Ok(result)
        });
        methods.add_async_method("del", |_, redis, key: String| async move {
            let mut redis = redis.get();
            let result: bool = redis
                .del(key)
                .await
                .map_err(|e| LuaError::ExternalError(Arc::new(e)))?;
            Ok(result)
        });

        methods.add_async_method("hget", |_, redis, args: (String, String)| async move {
            let (key, field) = args;
            let mut redis = redis.get();
            let result: String = redis
                .hget(key, field)
                .await
                .map_err(|e| LuaError::ExternalError(Arc::new(e)))?;
            Ok(result)
        });
        methods.add_async_method(
            "hset",
            |_, redis, args: (String, String, String)| async move {
                let (key, field, value) = args;
                let mut redis = redis.get();
                let result: bool = redis
                    .hset(key, field, value)
                    .await
                    .map_err(|e| LuaError::ExternalError(Arc::new(e)))?;
                Ok(result)
            },
        );
        methods.add_async_method("hdel", |_, redis, args: (String, String)| async move {
            let (key, field) = args;
            let mut redis = redis.get();
            let result: bool = redis
                .hdel(key, field)
                .await
                .map_err(|e| LuaError::ExternalError(Arc::new(e)))?;
            Ok(result)
        });
    }
}

impl LuaUserData for MqttClient {
    fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_async_method("subscribe", |_, mqtt, topic: String| async move {
            mqtt.subscribe(&topic).await;
            Ok(())
        });
    }
}

static ISSUES: std::sync::Mutex<BTreeSet<String>> = std::sync::Mutex::new(BTreeSet::new());

pub fn reset_issues() {
    let mut issues = ISSUES.lock().unwrap();
    issues.clear();
}

fn add_issue(issue: String) {
    let mut issues = ISSUES.lock().unwrap();
    issues.insert(issue);
}

pub fn issues() -> BTreeSet<String> {
    ISSUES.lock().unwrap().clone()
}
