use std::collections::BTreeSet;

use futures_util::future;
use models::hvac_request::HvacRequest;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use warp::{filters::BoxedFilter, path, Filter, Rejection, Reply};

use crate::{error::WebErrorExt, hvac::mixer::lua_controller::issues, StatePackage};

#[derive(Clone, Serialize, Deserialize)]
struct ScriptBody {
    script: String,
}

#[derive(Clone, Serialize, Deserialize)]
enum ValidationResponse {
    Error(String),
    Results {
        output: Option<HvacRequest>,
        issues: BTreeSet<String>,
    },
}

pub async fn routes(state: StatePackage<'_>) -> BoxedFilter<(impl Reply,)> {
    let scripts = { // GET /api/thermostat/lua/scripts
        let redis = state.redis.clone();
        warp::path("scripts")
            .and(path::end())
            .and(warp::get())
            .and_then(move || {
                let redis = redis.clone();
                async move {
                    let keys: Vec<String> = {
                        let mut redis = redis.get();
                        redis.hkeys("thermostat.lua.saved").await.reject_err()?
                    };
                    serde_json::to_string(&keys).reject_err()
                }
            })
    };

    let get_script = { // GET /api/thermostat/lua/scripts/<name>
        let redis = state.redis.clone();
        warp::path("scripts")
            .and(path::param())
            .and(path::end())
            .and(warp::get())
            .and_then(move |name: String| {
                let redis = redis.clone();
                async move {
                    let script: String = {
                        let mut redis = redis.get();
                        redis
                            .hget("thermostat.lua.saved", name)
                            .await
                            .reject_err()?
                    };
                    serde_json::to_string(&ScriptBody { script }).reject_err()
                }
            })
    };

    let put_script = {
        let redis = state.redis.clone();
        warp::path("scripts")
            .and(path::param())
            .and(path::end())
            .and(warp::put())
            .and(warp::body::json())
            .and_then(move |name: String, body: ScriptBody| {
                let redis = redis.clone();
                async move {
                    let mut redis = redis.get();
                    let () = redis
                        .hset("thermostat.lua.saved", name, body.script)
                        .await
                        .reject_err()?;
                    Ok::<_, Rejection>("ok".to_string())
                }
            })
    };

    let get_active_script = {
        let redis = state.redis.clone();
        warp::path("active_script")
        .and(path::end())
        .and(warp::get())
        .and_then(move || {
            let redis = redis.clone();
            async move {
                let script: String = {
                    let mut redis = redis.get();
                    redis.get("thermostat.lua.current").await.reject_err()?
                };
                serde_json::to_string(&ScriptBody { script }).reject_err()
            }
        })
    };

    let put_active_script = {
        let redis = state.redis.clone();
        let mixer = state.hvac.mixer.clone();
        warp::path("active_script")
            .and(path::end())
            .and(warp::put())
            .and(warp::body::json())
            .and_then(move |body: ScriptBody| {
                let redis = redis.clone();
                let mixer = mixer.clone();
                async move {
                    {
                        let mut redis = redis.get();
                        let () = redis.set("thermostat.lua.current", &body.script).await.reject_err()?;
                    }
                    mixer.state().set_active_lua_script(body.script).await.reject_err()?;
                    Ok::<_, Rejection>("ok".to_string())
                }
            })
    };

    let validate = {
        let mixer = state.hvac.mixer.clone();
        warp::path("validate")
            .and(path::end())
            .and(warp::post())
            .and(warp::body::json())
            .and_then(move |body: ScriptBody| {
                let mixer_state = mixer.state();
                async move {
                    let response = match mixer_state.validate_lua_script(body.script).await {
                        Ok((output, issues)) => ValidationResponse::Results { output, issues },
                        Err(e) => ValidationResponse::Error(e.to_string()),
                    };

                    serde_json::to_string(&response).reject_err()
                }
            })
    };

    let issues = warp::path("issues")
        .and(path::end())
        .and(warp::get())
        .and_then(move || {
            let issues = issues();
            future::ready(serde_json::to_string(&issues).reject_err())
        });

    scripts
        .or(get_script)
        .or(put_script)
        .or(get_active_script)
        .or(put_active_script)
        .or(validate)
        .or(issues)
        .boxed()
}
