use std::{collections::HashMap, str::FromStr};

use chrono::{DateTime, FixedOffset, NaiveDateTime};
use redis::AsyncCommands;
use serde::Serialize;
use warp::{
    filters::{path, BoxedFilter},
    Filter, Rejection, Reply,
};

use crate::{
    error::WebErrorExt,
    helpers::extract_redis_history_params,
    hvac::{PROBE_HISTORY}, StatePackage,
};

pub async fn routes(state: StatePackage<'_>) -> BoxedFilter<(impl Reply,)> {
    let index = {
        let probes = state.hvac.probes.clone();
        path::end().and(warp::get()).and_then(move || {
            let probes = probes.clone();
            async move { serde_json::to_string(&probes.keys().await).reject_err() }
        })
    };

    let temperature = {
        let probes = state.hvac.probes.clone();
        warp::path!(String / "temperature")
            .and(path::end())
            .and(warp::get())
            .and_then(move |probe: String| {
                let probes = probes.clone();
                async move {
                    let probe = probes
                        .get(&probe)
                        .await
                        .ok_or_else(|| warp::reject::not_found())?;

                    Ok::<_, Rejection>(probe.value().to_string())
                }
            })
    };

    let history = {
        let redis = state.redis.clone();
        warp::path!(String / "history")
            .and(warp::query::<HashMap<String, String>>())
            .and(path::end())
            .and(warp::get())
            .and_then(move |probe: String, query| {
                let redis = redis.clone();
                async move {
                    let (start, stop, offset) = extract_redis_history_params(&query).await?;

                    let mut redis = redis.get();
                    let history: Vec<String> = redis
                        .lrange(format!("{PROBE_HISTORY}:{probe}"), start, stop)
                        .await
                        .reject_err()?;

                    #[derive(Serialize)]
                    struct HistoryEntry {
                        time: DateTime<FixedOffset>,
                        temp: f64,
                    }

                    let history: Vec<_> = history
                        .into_iter()
                        .filter_map(|s| {
                            let mut split = s.split(':');
                            let time_i =
                                split.next().and_then(|s| i64::from_str_radix(s, 10).ok())?;
                            let temp = split.next().and_then(|s| f64::from_str(s).ok())?;
                            Some(HistoryEntry {
                                time: DateTime::from_utc(
                                    NaiveDateTime::from_timestamp_opt(
                                        time_i / 1000,
                                        (time_i % 1000) as u32 * 1_000_000,
                                    )?,
                                    offset,
                                ),
                                temp,
                            })
                        })
                        .collect();

                    serde_json::to_string(&history).reject_err()
                }
            })
    };

    index.or(temperature).or(history).boxed()
}
