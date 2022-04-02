use std::{collections::HashMap, str::FromStr};

use chrono::FixedOffset;
use warp::{reject::Reject, Rejection};

pub async fn extract_redis_history_params<'p>(
    query: &HashMap<String, String>,
) -> Result<(isize, isize, FixedOffset), Rejection> {
    #[derive(Debug, Copy, Clone)]
    struct MissingOrInvalidParameter(&'static str);
    impl Reject for MissingOrInvalidParameter {}

    let start = query
        .get("start")
        .and_then(|s| isize::from_str_radix(s, 10).ok())
        .ok_or_else(|| warp::reject::custom(MissingOrInvalidParameter("start")))?;

    let stop = query
        .get("stop")
        .and_then(|s| isize::from_str_radix(s, 10).ok())
        .ok_or_else(|| warp::reject::custom(MissingOrInvalidParameter("stop")))?;

    let offset = FixedOffset::east_opt(
        (query
            .get("tzoff")
            .and_then(|s| f64::from_str(s).ok())
            .unwrap_or(0.0)
            * 3600.0) as i32,
    )
    .ok_or_else(|| warp::reject::custom(MissingOrInvalidParameter("stop")))?;

    Ok((start, stop, offset))
}
