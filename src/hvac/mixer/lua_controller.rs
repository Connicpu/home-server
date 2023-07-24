use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use chrono::NaiveTime;
use mlua::prelude::*;
use models::hvac_request::HvacRequest;
use redis::AsyncCommands;
use tokio::sync::Mutex;

use crate::{hvac::{probe::Probe, Probes}, RedisConn};

use super::MixerState;

#[derive(Clone, Default)]
pub struct LuaController {
    state: Arc<Mutex<LuaControllerState>>,
}

impl LuaController {
    pub async fn is_loaded(&self) -> bool {
        let state = self.state.lock().await;
        state.is_loaded()
    }

    pub async fn validate(&self, script: &str, mixer: &MixerState) -> anyhow::Result<(Option<HvacRequest>, BTreeSet<String>)> {
        // Lock the state so the issues won't fill up
        let mut _state = self.state.lock().await;

        reset_issues();
        let mut temp_state = LuaControllerState::default();
        temp_state.load(script).await?;
        let result = temp_state.evaluate(mixer).await;

        Ok((result, issues()))
    }

    pub async fn load(&self, script: &str) -> anyhow::Result<()> {
        let mut state = self.state.lock().await;
        state.load(script).await
    }

    pub async fn evaluate(&self, mixer: &MixerState) -> Option<HvacRequest> {
        let mut state = self.state.lock().await;
        state.evaluate(mixer).await
    }

    pub async fn load_redis(&self, redis: &RedisConn) -> anyhow::Result<()> {
        let script: String = {
            let mut redis = redis.get();
            redis.get("thermostat.lua.current").await?
        };

        self.load(&script).await
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

    async fn load(&mut self, script: &str) -> anyhow::Result<()> {
        self.lua.load(script).exec_async().await?;

        Ok(())
    }

    async fn evaluate(&mut self, mixer: &MixerState) -> Option<HvacRequest> {
        let evaluate: LuaFunction = self.lua.globals().get("evaluate").ok()?;
        let result: Option<String> = evaluate.call_async(mixer.clone()).await.ok()?;

        result.and_then(|res| HvacRequest::from_payload(res.as_bytes()))
    }
}

impl LuaUserData for MixerState {
    fn add_fields<'lua, F: LuaUserDataFields<'lua, Self>>(fields: &mut F) {
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
                        lua.inspect_stack(0).map(|d| d.curr_line()).unwrap_or(-1)
                    ));
                    continue
                };
                let Ok(time) = NaiveTime::parse_from_str(&time_str, "%H:%M") else {
                    add_issue(format!(
                        "[src:{}] Timed program keys must be in 'HH:MM' format. Found {time_str}",
                        lua.inspect_stack(0).map(|d| d.curr_line()).unwrap_or(-1)
                    ));
                    continue
                };
                program_table.insert(time, func);
            }
            if program_table.is_empty() {
                add_issue(format!(
                    "[src:{}] Empty program table",
                    lua.inspect_stack(0).map(|d| d.curr_line()).unwrap_or(-1)
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
