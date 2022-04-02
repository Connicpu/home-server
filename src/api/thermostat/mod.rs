use std::collections::HashMap;

use chrono::{DateTime, FixedOffset, NaiveDateTime};
use redis::AsyncCommands;
use serde::Serialize;
use warp::{
    filters::{path, BoxedFilter},
    Filter, Reply,
};

use crate::{
    error::WebErrorExt,
    helpers::extract_redis_history_params,
    hvac::{mixer::HvacRequest, HvacState, PINSTATE_HISTORY},
    RedisConn,
};

pub mod probes;
pub mod pulse_override;
pub mod rules;

pub async fn routes(redis: &RedisConn, hvac: &HvacState) -> BoxedFilter<(impl Reply,)> {
    let probes = warp::path("probes").and(probes::routes(redis, hvac).await);
    let rules = warp::path("rules").and(rules::routes(redis, hvac).await);
    let pulse_override = warp::path("pulse_override").and(pulse_override::routes(hvac).await);

    let pinstate_history = pinstate_history(redis);

    probes
        .or(rules)
        .or(pulse_override)
        .or(pinstate_history)
        .boxed()
}

fn pinstate_history(redis: &RedisConn) -> BoxedFilter<(impl Reply,)> {
    let redis = redis.clone();
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
                                NaiveDateTime::from_timestamp(
                                    time_i / 1000,
                                    (time_i % 1000) as u32 * 1_000_000,
                                ),
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
