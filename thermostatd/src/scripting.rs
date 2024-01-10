use std::{
    cmp::Ordering,
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use chrono::{DateTime, NaiveTime, Utc};
use mlua::prelude::*;
use models::{hvac_request::HvacRequest, thermostatd::OneshotOrdering};
use redis::AsyncCommands;
use rumqttc::QoS;

use crate::{
    channels,
    mqtt::{publish_oneshot_override, publish_timed_override},
    CommonState,
};

pub async fn run_script_loop(
    mqtt: rumqttc::AsyncClient,
    redis: redis::aio::ConnectionManager,
    state: Arc<CommonState>,
) -> anyhow::Result<()> {
    let mut lua = Lua::new();
    let mut last_script_timestamp: DateTime<Utc> = Default::default();
    let script_state = ScriptState {
        mqtt: mqtt.clone(),
        redis,
        state: state.clone(),
    };

    // Give MQTT time to initialize
    tokio::time::sleep(Duration::from_secs(1).into()).await;

    let mut next_evaluation = Instant::now();
    loop {
        let next_tick = Instant::now() + Duration::from_secs(1);

        // Check if the script should be reloaded
        let state_script = state.script.get();
        'load_script: {
            if last_script_timestamp != state_script.1 {
                last_script_timestamp = state_script.1;
                if let Err(e) = test_script(&state_script.0, &script_state).await {
                    eprintln!("Error loading script during test_script\n{e:?}");
                    mqtt.publish(
                        channels::SCRIPT_DATA_ERROR,
                        QoS::ExactlyOnce,
                        true,
                        serde_json::json!({
                            "success": false,
                            "error_at": "test_script",
                            "error": format!("{e:?}"),
                        })
                        .to_string(),
                    )
                    .await?;
                    break 'load_script;
                }
                if let Err(e) = load_script(&mut lua, &state_script.0).await {
                    eprintln!("Error loading script\n{e:?}");
                    mqtt.publish(
                        channels::SCRIPT_DATA_ERROR,
                        QoS::ExactlyOnce,
                        true,
                        serde_json::json!({
                            "success": false,
                            "error_at": "load_script",
                            "error": format!("{e:?}"),
                        })
                        .to_string(),
                    )
                    .await?;
                    break 'load_script;
                }
                if let Err(e) = init_script(&mut lua, &script_state).await {
                    eprintln!("Error initializing script\n{e:?}");
                    mqtt.publish(
                        channels::SCRIPT_DATA_ERROR,
                        QoS::ExactlyOnce,
                        true,
                        serde_json::json!({
                            "success": false,
                            "error_at": "init_script",
                            "error": format!("{e:?}"),
                        })
                        .to_string(),
                    )
                    .await?;
                    break 'load_script;
                }
                mqtt.publish(
                    channels::SCRIPT_DATA_ERROR,
                    QoS::ExactlyOnce,
                    true,
                    serde_json::json!({
                        "success": true,
                    })
                    .to_string(),
                )
                .await?;
            }
        }

        if let Err(e) = tick_script(&mut lua, &script_state).await {
            eprintln!("Script tick error{e:?}");
            mqtt.publish(
                channels::SCRIPT_DATA_ERROR,
                QoS::ExactlyOnce,
                true,
                serde_json::json!({
                    "success": false,
                    "error_at": "tick_script",
                    "error": format!("{e:?}"),
                })
                .to_string(),
            )
            .await?;
        }

        if next_evaluation < Instant::now() {
            let mut next_call = None;

            if next_call.is_none()
                && let Some(timed_override) = *state.timed_override.get()
            {
                if timed_override.expiration > Utc::now() {
                    next_call = Some(timed_override.command);
                } else {
                    state.timed_override.take();
                    publish_timed_override(&mqtt, &state).await?;
                }
            }

            if next_call.is_none()
                && let Some(ref oneshot_override) = *state.oneshot_override.get()
            {
                if let Some(currtemp) = state.probe_values.get().get(&oneshot_override.probe) {
                    match (
                        oneshot_override.comparison,
                        currtemp.partial_cmp(&(oneshot_override.setpoint as f64)),
                    ) {
                        (OneshotOrdering::Less, Some(Ordering::Less)) => {
                            next_call = Some(oneshot_override.command);
                        }
                        (OneshotOrdering::Greater, Some(Ordering::Greater)) => {
                            next_call = Some(oneshot_override.command);
                        }
                        _ => {
                            state.oneshot_override.take();
                            publish_oneshot_override(&mqtt, &state).await?;
                        }
                    }
                } else {
                    state.oneshot_override.take();
                    publish_oneshot_override(&mqtt, &state).await?;
                }
            }

            if next_call.is_none() {
                match evaluate_script(&mut lua, &script_state).await {
                    Ok(Some(call)) => next_call = HvacRequest::from_string(call),
                    Ok(None) => {}
                    Err(e) => {
                        eprintln!("Script evaluate error{e:?}");
                        mqtt.publish(
                            channels::SCRIPT_DATA_ERROR,
                            QoS::ExactlyOnce,
                            true,
                            serde_json::json!({
                                "success": false,
                                "error_at": "evaluate_script",
                                "error": format!("{e:?}"),
                            })
                            .to_string(),
                        )
                        .await?;
                    }
                };
            }

            if let Some(next_call) = next_call {
                if next_call != *state.last_call.get() {
                    println!("New call: {next_call}");
                    state.last_call.set(Arc::new(next_call));
                }
            }

            mqtt.publish(
                channels::HVAC_REMOTESTATE_SET,
                QoS::AtLeastOnce,
                false,
                state.last_call.get().payload_str(),
            )
            .await?;

            next_evaluation = Instant::now() + Duration::from_secs(10);
        }

        tokio::time::sleep_until(next_tick.into()).await;
    }
}

pub async fn test_script(script: &str, state: &ScriptState) -> anyhow::Result<()> {
    let mut lua = Lua::new();
    load_script(&mut lua, script).await?;
    evaluate_script(&mut lua, state).await?;
    Ok(())
}

async fn load_script(lua: &mut Lua, script: &str) -> anyhow::Result<()> {
    lua.load(script).exec_async().await?;
    Ok(())
}

async fn init_script(lua: &mut Lua, state: &ScriptState) -> anyhow::Result<()> {
    if let Ok(init) = lua.globals().get::<_, LuaFunction>("init") {
        let () = init.call_async(state.clone()).await?;
    }
    Ok(())
}

async fn tick_script(lua: &mut Lua, state: &ScriptState) -> anyhow::Result<()> {
    if let Ok(tick) = lua.globals().get::<_, LuaFunction>("tick") {
        let () = tick.call_async(state.clone()).await?;
    }
    Ok(())
}

async fn evaluate_script(lua: &mut Lua, state: &ScriptState) -> anyhow::Result<Option<String>> {
    if let Ok(evaluate) = lua.globals().get::<_, LuaFunction>("evaluate") {
        let result: Option<String> = evaluate.call_async(state.clone()).await?;
        Ok(result)
    } else {
        Ok(None)
    }
}

#[derive(Clone)]
pub struct ScriptState {
    pub mqtt: rumqttc::AsyncClient,
    pub redis: redis::aio::ConnectionManager,
    pub state: Arc<CommonState>,
}

impl LuaUserData for ScriptState {
    /// Adds custom fields specific to this userdata.
    fn add_fields<'lua, F: LuaUserDataFields<'lua, Self>>(fields: &mut F) {
        fields.add_field_method_get("probes", |_, ss| {
            Ok(ProbesProxy {
                state: ss.state.clone(),
            })
        });
        fields.add_field_method_get("mqtt", |_, ss| {
            Ok(MqttProxy {
                mqtt: ss.mqtt.clone(),
                state: ss.state.clone(),
            })
        });
        fields.add_field_method_get("redis", |_, ss| {
            Ok(RedisProxy {
                redis: ss.redis.clone(),
            })
        });
        fields.add_field_method_get("mode", |_, ss| {
            Ok(ss.state.mode.get().payload_str().to_string())
        })
    }

    /// Adds custom methods and operators specific to this userdata.
    fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_async_method("timed_program", |lua, _this, program: LuaTable| async move {
            let mut program_table = BTreeMap::new();
            for pair in program.pairs::<String, LuaFunction>() {
                let Ok((time_str, func)) = pair else {
                    return Err(anyhow::anyhow!(
                        "[src:{}] Timed program must be passed a table mapping time strings to functions",
                        lua.inspect_stack(1).map(|d| d.curr_line()).unwrap_or(-1)
                    )).luafy_error();
                };
                let Ok(time) = NaiveTime::parse_from_str(&time_str, "%H:%M") else {
                    return Err(anyhow::anyhow!(
                        "[src:{}] Timed program keys must be in 'HH:MM' format. Found {time_str}",
                        lua.inspect_stack(1).map(|d| d.curr_line()).unwrap_or(-1)
                    )).luafy_error();
                };
                program_table.insert(time, func);
            }
            if program_table.is_empty() {
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

#[derive(Clone)]
struct MqttProxy {
    mqtt: rumqttc::AsyncClient,
    state: Arc<CommonState>,
}

impl LuaUserData for MqttProxy {
    /// Adds custom fields specific to this userdata.
    fn add_fields<'lua, F: LuaUserDataFields<'lua, Self>>(_fields: &mut F) {}

    /// Adds custom methods and operators specific to this userdata.
    fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_async_method("subscribe", |_, mp, topic: String| async move {
            println!("Subscribing to {topic:?} at request of lua script");
            mp.state
                .retained_keys
                .write()
                .unwrap()
                .entry(topic.clone())
                .or_default();
            mp.mqtt
                .subscribe(topic.clone(), QoS::AtLeastOnce)
                .await
                .with_context(|| format!("while subscribing to {topic}"))
                .luafy_error()?;
            Ok(())
        });
        methods.add_meta_method("__index", |_, mp, topic: String| {
            Ok(mp.state.retained_keys.read().unwrap().get(&topic).cloned())
        });
    }
}

#[derive(Clone)]
struct RedisProxy {
    redis: redis::aio::ConnectionManager,
}

impl LuaUserData for RedisProxy {
    /// Adds custom fields specific to this userdata.
    fn add_fields<'lua, F: LuaUserDataFields<'lua, Self>>(_fields: &mut F) {}

    /// Adds custom methods and operators specific to this userdata.
    fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_async_method("get", |_, mut rp, key: String| async move {
            let value: String = rp
                .redis
                .get(&key)
                .await
                .with_context(|| format!("Redis operation: GET {key:?}"))
                .luafy_error()?;
            Ok(value)
        });
    }
}

#[derive(Clone)]
struct ProbesProxy {
    state: Arc<CommonState>,
}

impl LuaUserData for ProbesProxy {
    /// Adds custom fields specific to this userdata.
    fn add_fields<'lua, F: LuaUserDataFields<'lua, Self>>(_fields: &mut F) {}

    /// Adds custom methods and operators specific to this userdata.
    fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_meta_method("__index", |_, pp, probe: String| {
            Ok(pp.state.probe_values.get().get(&probe).cloned())
        });
    }
}

trait LuaErrorHelper {
    type Output;
    fn luafy_error(self) -> Self::Output;
}

impl<T, E> LuaErrorHelper for Result<T, E>
where
    E: Into<anyhow::Error>,
{
    type Output = Result<T, LuaError>;
    fn luafy_error(self) -> Self::Output {
        self.map_err(|e| Box::<dyn std::error::Error + Send + Sync>::from(e.into()))
            .map_err(|e| LuaError::ExternalError(Arc::from(e)))
    }
}
