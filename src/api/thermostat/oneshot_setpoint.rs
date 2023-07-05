use std::future::ready;

use warp::{filters::BoxedFilter, path, Filter, Rejection, Reply};

use crate::{
    error::WebErrorExt, hvac::mixer::oneshot_setpoint::OneshotSetpointState, StatePackage,
};

pub async fn routes(state: StatePackage<'_>) -> BoxedFilter<(impl Reply,)> {
    let index = {
        let hvac = state.hvac.clone();
        path::end().and(warp::get()).and_then(move || {
            let state = hvac.mixer.state().oneshot_setpoint.get();
            ready(serde_json::to_string(&state).reject_err())
        })
    };

    let put = {
        let hvac = state.hvac.clone();
        path::end()
            .and(warp::put())
            .and(warp::body::json::<Option<OneshotSetpointState>>())
            .and_then(move |new_state| {
                let state = hvac.mixer.state();
                async move {
                    state.oneshot_setpoint.set(new_state);
                    Ok::<_, Rejection>("ok".to_string())
                }
            })
    };

    index.or(put).boxed()
}
