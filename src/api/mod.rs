use http::StatusCode;
use warp::{reply, filters::BoxedFilter, Filter, Reply, Rejection};

use crate::{hvac::HvacState, mqtt::MqttClient, RedisConn};

use self::auth::AuthFailed;

pub mod atticfan;
pub mod auth;
pub mod thermostat;

pub async fn routes(
    mqtt: &MqttClient,
    redis: &RedisConn,
    hvac: &HvacState,
) -> BoxedFilter<(impl Reply,)> {
    let auth = warp::path("auth").and(auth::routes(redis.clone()).await);

    let atticfan = warp::path("atticfan").and(auth::with_auth(1)).and(atticfan::routes(mqtt).await);
    let thermostat = warp::path("thermostat").and(auth::with_auth(1)).and(thermostat::routes(redis, hvac).await);

    let authed_routes = atticfan.or(thermostat);
    auth.or(authed_routes)
        .recover(|rejection: Rejection| async move {
            if let Some(fail) = rejection.find::<AuthFailed>() {
                let mut resp = reply::json(fail).into_response();
                *resp.status_mut() = StatusCode::FORBIDDEN;
                Ok(resp)
            } else {
                Err(rejection)
            }
        })
        .boxed()
}
