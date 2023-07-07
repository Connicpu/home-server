#![feature(async_fn_in_trait)]

use hvac::HvacState;
use mqtt::MqttClient;
use rumqttc::MqttOptions;
use std::future::Future;
use std::time::Duration;

pub use self::redis::RedisConn;
use api::atticfan::FanState;

#[cfg(feature = "routes")]
pub mod api;
pub mod error;
pub mod helpers;
pub mod hvac;
pub mod mqtt;
pub mod redis;

const PORT: u16 = 3030;
const MQTT_HOST: &str = "raspberrypi.local";
const MQTT_PORT: u16 = 1883;
const REDIS_HOST: &str = "nas.iot.connieh.com";
const REDIS_PORT: u16 = 6379;

#[derive(Copy, Clone)]
pub struct StatePackage<'a> {
    mqtt: &'a MqttClient,
    redis: &'a RedisConn,
    hvac: &'a HvacState,
    fan: &'a FanState,
}

#[cfg(feature = "routes")]
pub async fn run_server() -> anyhow::Result<()> {
    let mqtt = {
        let mut options = MqttOptions::new("pi-management-server", MQTT_HOST, MQTT_PORT);
        options.set_keep_alive(Duration::from_secs(5));
        mqtt::init(options)
    };

    let redis: RedisConn = RedisConn::open(REDIS_HOST, REDIS_PORT).await?;
    let fan_state = FanState::default();
    let hvac = hvac::initialize(&mqtt, &redis, &fan_state).await?;

    let state = StatePackage {
        mqtt: &mqtt,
        redis: &redis,
        hvac: &hvac,
        fan: &fan_state
    };

    let api = api::routes(state).await;

    warp::serve(api).run(([0, 0, 0, 0], PORT)).await;

    Ok(())
}

#[cfg(tokio_unstable)]
#[track_caller]
fn spawn(name: &str, future: impl Future<Output = impl Send + 'static> + Send + 'static) {
    tokio::task::Builder::new().name(name).spawn(future);
}
#[cfg(not(tokio_unstable))]
#[track_caller]
fn spawn(_name: &str, future: impl Future<Output = impl Send + 'static> + Send + 'static) {
    tokio::spawn(future);
}
