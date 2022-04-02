use std::future::ready;

use warp::{
    filters::{path, BoxedFilter},
    Filter, Rejection, Reply,
};

use crate::{
    error::WebErrorExt,
    hvac::{mixer::override_pulse::OverridePulseState, HvacState},
};

pub async fn routes(hvac: &HvacState) -> BoxedFilter<(impl Reply,)> {
    let index = {
        let hvac = hvac.clone();
        path::end().and(warp::get()).and_then(move || {
            let state = hvac.mixer.state().override_pulse.get();
            ready(serde_json::to_string(&state).reject_err())
        })
    };

    let put = {
        let hvac = hvac.clone();
        path::end()
            .and(warp::put())
            .and(warp::body::json::<Option<OverridePulseState>>())
            .and_then(move |new_state| {
                let state = hvac.mixer.state();
                async move {
                    state.override_pulse.set(new_state);
                    Ok::<_, Rejection>("ok".to_string())
                }
            })
    };

    index.or(put).boxed()
}
