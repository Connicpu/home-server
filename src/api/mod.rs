use http::StatusCode;
use warp::{filters::BoxedFilter, reply, Filter, Rejection, Reply};

use crate::StatePackage;

use self::auth::AuthFailed;

pub mod atticfan;
pub mod auth;
pub mod thermostat;

pub async fn routes(state: StatePackage<'_>) -> BoxedFilter<(impl Reply,)> {
    let auth = warp::path("auth").and(auth::routes(state).await);

    let atticfan = warp::path("atticfan")
        .and(auth::with_auth(1))
        .and(atticfan::routes(state).await);
    let thermostat = warp::path("thermostat")
        .and(auth::with_auth(1))
        .and(thermostat::routes(state).await);

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
