#![feature(if_let_guard, let_chains)]

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use arc_cell::ArcCell;
use chrono::{DateTime, Utc};
use models::{
    hvac_request::HvacRequest,
    thermostatd::{OneshotOverride, TimedOverride},
};
use redis::AsyncCommands;
use rumqttc::{MqttOptions, QoS};

use crate::{mqtt::run_mqtt_eventloop, scripting::run_script_loop};

mod mqtt;
mod scripting;

const MQTT_CLIENT_NAME: &str = "thermostatd";
const DEFAULT_SCRIPT: &str = include_str!("default_script.lua");

mod channels {
    pub const HVAC_REMOTESTATE_SET: &str = "test/thermostat/hvac/remotestate/set";
    pub const HVAC_MODE: &str = "home/thermostat/hvac/mode";
    pub const REMOTESTATE: &str = "home/thermostat/hvac/remotestate";

    pub const SCRIPT_DATA: &str = "home/thermostatd/script";
    pub const SCRIPT_DATA_GET: &str = "home/thermostatd/script/get";
    pub const SCRIPT_DATA_SET: &str = "home/thermostatd/script/set";
    pub const SCRIPT_DATA_ERROR: &str = "home/thermostatd/script/error";
    pub const SCRIPT_DATA_TEST: &str = "home/thermostatd/script/test";
    pub const SCRIPT_DATA_TEST_ERROR: &str = "home/thermostatd/script/test/error";

    pub const TIMED_OVERRIDE: &str = "home/thermostatd/timed_override";
    pub const TIMED_OVERRIDE_GET: &str = "home/thermostatd/timed_override/get";
    pub const TIMED_OVERRIDE_SET: &str = "home/thermostatd/timed_override/set";
    pub const TIMED_OVERRIDE_ERROR: &str = "home/thermostatd/timed_override/error";

    pub const ONESHOT_OVERRIDE: &str = "home/thermostatd/oneshot_override";
    pub const ONESHOT_OVERRIDE_GET: &str = "home/thermostatd/oneshot_override/get";
    pub const ONESHOT_OVERRIDE_SET: &str = "home/thermostatd/oneshot_override/set";
    pub const ONESHOT_OVERRIDE_ERROR: &str = "home/thermostatd/oneshot_override/error";
}

mod keys {
    pub const SAVED_SCRIPT: &str = "thermostatd.script";
    pub const TIMED_OVERRIDE: &str = "thermostatd.timed_override";
    pub const ONESHOT_OVERRIDE: &str = "thermostatd.oneshot_override";
    pub const PROBE_ENDPOINTS: &str = "thermostat.probes";
}

#[derive(Default, Clone)]
struct CommonState {
    mode: ArcCell<HvacRequest>,
    last_call: ArcCell<HvacRequest>,
    timed_override: ArcCell<Option<TimedOverride>>,
    oneshot_override: ArcCell<Option<OneshotOverride>>,
    script: ArcCell<(String, DateTime<Utc>)>,
    probe_values: ArcCell<HashMap<String, f64>>,
    retained_keys: Arc<RwLock<HashMap<String, String>>>,
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    let mut last_restart = None::<Instant>;
    let mut pileon_fails = 0;
    loop {
        if let Err(err) = run_thermostat().await {
            eprintln!("Error encountered: {err}");
        };

        if let Some(last_restart_time) = last_restart {
            let now = Instant::now();
            if (now - last_restart_time) < Duration::from_secs(60) {
                pileon_fails += 1;
                tokio::time::sleep(Duration::from_secs(pileon_fails * pileon_fails)).await;
            }
        }

        last_restart = Some(Instant::now());
        eprintln!("Restarting thermostatd");
    }
}

async fn run_thermostat() -> anyhow::Result<()> {
    println!("Starting!");
    let (mqtt, mqtt_eventloop) = open_mqtt()?;
    let redis = open_redis().await?;

    let state: Arc<CommonState> = Default::default();
    initialize_state(mqtt.clone(), redis.clone(), state.clone()).await?;

    tokio::try_join!(
        run_mqtt_eventloop(mqtt.clone(), redis.clone(), mqtt_eventloop, state.clone()),
        run_script_loop(mqtt.clone(), redis.clone(), state.clone()),
    )?;

    Ok(())
}

async fn initialize_state(
    mqtt: rumqttc::AsyncClient,
    mut redis: redis::aio::ConnectionManager,
    state: Arc<CommonState>,
) -> anyhow::Result<()> {
    println!("Initializing state");
    let saved_script: Option<String> = redis.get(keys::SAVED_SCRIPT).await?;
    state.script.set(Arc::new((
        saved_script.unwrap_or_else(|| DEFAULT_SCRIPT.into()),
        Utc::now(),
    )));
    mqtt.publish(
        channels::SCRIPT_DATA,
        QoS::ExactlyOnce,
        true,
        state.script.get().0.as_bytes(),
    )
    .await?;

    let timed_override_data: Option<String> = redis.get(keys::TIMED_OVERRIDE).await?;
    if let Some(timed_override_data) = timed_override_data {
        if let Ok(timed_override) =
            serde_json::from_str::<Option<TimedOverride>>(&timed_override_data)
        {
            state.timed_override.set(Arc::new(timed_override));
        } else {
            redis.del(keys::TIMED_OVERRIDE).await?;
        }
    }
    mqtt.publish(
        channels::TIMED_OVERRIDE,
        QoS::ExactlyOnce,
        true,
        serde_json::to_string(&*state.timed_override.get())?,
    )
    .await?;

    let oneshot_override_data: Option<String> = redis.get(keys::ONESHOT_OVERRIDE).await?;
    if let Some(oneshot_override_data) = oneshot_override_data {
        if let Ok(oneshot_override) =
            serde_json::from_str::<Option<OneshotOverride>>(&oneshot_override_data)
        {
            state.oneshot_override.set(Arc::new(oneshot_override));
        } else {
            redis.del(keys::ONESHOT_OVERRIDE).await?;
        }
    }

    Ok(())
}

fn open_mqtt() -> anyhow::Result<(rumqttc::AsyncClient, rumqttc::EventLoop)> {
    let (host, port) = (std::env::var("MQTT_HOST")?, std::env::var("MQTT_PORT")?);
    let (user, pass) = (std::env::var("MQTT_USER")?, std::env::var("MQTT_PASS")?);
    let mut options = MqttOptions::new(MQTT_CLIENT_NAME, host, port.parse()?);
    options.set_credentials(user, pass);
    options.set_keep_alive(Duration::from_secs(5));
    Ok(rumqttc::AsyncClient::new(options, 256))
}

async fn open_redis() -> anyhow::Result<redis::aio::ConnectionManager> {
    let client = redis::Client::open(std::env::var("REDIS_ADDR")?)?;
    let cm = redis::aio::ConnectionManager::new(client).await?;
    Ok(cm)
}
