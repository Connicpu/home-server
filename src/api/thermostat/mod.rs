use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use chrono::{DateTime, FixedOffset, NaiveDateTime};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use warp::{
    filters::{path, BoxedFilter},
    Filter, Reply,
};

use crate::{
    error::WebErrorExt,
    helpers::extract_redis_history_params,
    hvac::{mixer::HvacRequest, PINSTATE_HISTORY},
    StatePackage,
};

pub mod oneshot_setpoint;
pub mod probes;
pub mod pulse_override;
pub mod rules;

pub async fn routes(state: StatePackage<'_>) -> BoxedFilter<(impl Reply,)> {
    let oneshot_setpoint =
        warp::path("oneshot_setpoint").and(oneshot_setpoint::routes(state).await);
    let probes = warp::path("probes").and(probes::routes(state).await);
    let rules = warp::path("rules").and(rules::routes(state).await);
    let pulse_override = warp::path("pulse_override").and(pulse_override::routes(state).await);

    let pinstate_history = pinstate_history(state);
    let mode = mode(state);

    oneshot_setpoint
        .or(probes)
        .or(rules)
        .or(pulse_override)
        .or(pinstate_history)
        .or(mode)
        .boxed()
}

fn pinstate_history(state: StatePackage<'_>) -> BoxedFilter<(impl Reply,)> {
    let redis = state.redis.clone();
    warp::path("pinstate")
        .and(warp::path("history"))
        .and(warp::query::<HashMap<String, String>>())
        .and(path::end())
        .and(warp::get())
        .and_then(move |query| {
            let redis = redis.clone();
            async move {
                let (start, stop, offset) = extract_redis_history_params(&query).await?;

                let mut redis = redis.get();
                let history: Vec<String> = redis
                    .lrange(PINSTATE_HISTORY, start, stop)
                    .await
                    .reject_err()?;

                #[derive(Serialize)]
                struct HistoryEntry {
                    time: DateTime<FixedOffset>,
                    state: HvacRequest,
                }

                let history: Vec<_> = history
                    .into_iter()
                    .filter_map(|s| {
                        let mut split = s.split(':');
                        let state = split
                            .next()
                            .and_then(|s| HvacRequest::from_payload(s.as_bytes()))?;
                        let time_i = split.next().and_then(|s| i64::from_str_radix(s, 10).ok())?;
                        Some(HistoryEntry {
                            time: DateTime::from_utc(
                                NaiveDateTime::from_timestamp_opt(
                                    time_i / 1000,
                                    (time_i % 1000) as u32 * 1_000_000,
                                )?,
                                offset,
                            ),
                            state,
                        })
                    })
                    .collect();

                serde_json::to_string(&history).reject_err()
            }
        })
        .boxed()
}

#[derive(Serialize, Deserialize, Clone)]
struct HvacModeState {
    mode: HvacRequest,
}

fn mode(state: StatePackage<'_>) -> BoxedFilter<(impl Reply,)> {
    let mode = state.hvac.hvac_mode.clone();
    let get = warp::get().and_then(move || {
        let mode = mode.clone();
        async move { serde_json::to_string(&HvacModeState { mode: mode.load() }).reject_err() }
    });

    let mode = state.hvac.hvac_mode.clone();
    let mqtt = state.mqtt.clone();
    let set = warp::put()
        .and(warp::body::json::<HvacModeState>())
        .and_then(move |new_state: HvacModeState| {
            let mode = mode.clone();
            let mqtt = mqtt.clone();
            async move {
                mqtt.publish("home/thermostat/hvac/mode/set", new_state.mode.payload())
                    .await;
                let begin = Instant::now();
                const MAX_TIME: Duration = Duration::from_secs(5);
                while mode.load() != new_state.mode {
                    tokio::time::sleep(Duration::from_millis(100)).await;

                    if Instant::now() - begin > MAX_TIME {
                        return Err(anyhow::anyhow!("timeout")).reject_err();
                    }
                }

                serde_json::to_string(&new_state).reject_err()
            }
        });

    warp::path("mode").and(path::end()).and(get.or(set)).boxed()
}
