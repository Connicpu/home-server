use std::sync::Arc;

use tokio::sync::RwLock;
use warp::{filters::BoxedFilter, Filter, Reply};

use crate::mqtt::MqttClient;

#[derive(Default)]
struct FanState {
    fan0: bool,
    fan1: bool,
}

pub async fn routes(mqtt: &MqttClient) -> BoxedFilter<(impl Reply,)> {
    let fan_state = Arc::new(RwLock::new(FanState::default()));

    // Handle updating the fan state from MQTT
    {
        let fan_state = fan_state.clone();
        mqtt.subscribe("home/atticfan/state").await;
        mqtt.handle("home/atticfan/state", move |_topic, payload| {
            if payload.len() != 2 {
                return;
            }

            let fan = payload[0].wrapping_sub(b'0');
            let val = payload[1] == b't';

            let state = fan_state.clone();
            tokio::task::spawn(async move {
                let mut state = state.write().await;
                match fan {
                    0 => state.fan0 = val,
                    1 => state.fan1 = val,
                    _ => (),
                }
            });
        })
        .await;

        // Make sure we're fresh
        mqtt.publish("home/atticfan/getstate", b"0").await;
        mqtt.publish("home/atticfan/getstate", b"1").await;
    }

    // Handle requests for the current known fan state
    let getstate = {
        let fan_state = fan_state.clone();
        warp::path!("getstate" / i32).and_then(move |fan: i32| {
            let fan_state = fan_state.clone();
            async move {
                let state = fan_state.read().await;
                let val = match fan {
                    0 => state.fan0,
                    1 => state.fan1,
                    _ => return Err(warp::reject::not_found()),
                };

                Ok(val.to_string())
            }
        })
    };

    let setstate = {
        let mqtt = mqtt.clone();
        warp::path!("setstate" / i32 / bool).and_then(move |fan, val| {
            let mqtt = mqtt.clone();
            async move {
                match fan {
                    0 | 1 => {}
                    _ => return Err(warp::reject::not_found()),
                }

                let payload = [b'0' + fan as u8, if val { b't' } else { b'f' }];
                mqtt.publish("home/atticfan/setstate", &payload).await;

                Ok("ok")
            }
        })
    };

    getstate.or(setstate).boxed()
}
