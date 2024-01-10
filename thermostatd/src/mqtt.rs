use std::{collections::BTreeMap, sync::Arc};

use chrono::Utc;
use models::{
    hvac_request::HvacRequest,
    thermostatd::{OneshotOverride, TimedOverride},
};
use redis::AsyncCommands;
use rumqttc::QoS;

use crate::{
    channels, keys,
    scripting::{test_script, ScriptState},
    CommonState,
};

pub async fn run_mqtt_eventloop(
    mqtt: rumqttc::AsyncClient,
    mut redis: redis::aio::ConnectionManager,
    mut mqtt_eventloop: rumqttc::EventLoop,
    state: Arc<CommonState>,
) -> anyhow::Result<()> {
    mqtt.subscribe(channels::SCRIPT_DATA_GET, QoS::ExactlyOnce)
        .await?;
    mqtt.subscribe(channels::SCRIPT_DATA_SET, QoS::ExactlyOnce)
        .await?;
    mqtt.subscribe(channels::TIMED_OVERRIDE_GET, QoS::ExactlyOnce)
        .await?;
    mqtt.subscribe(channels::TIMED_OVERRIDE_SET, QoS::ExactlyOnce)
        .await?;
    mqtt.subscribe(channels::ONESHOT_OVERRIDE_GET, QoS::ExactlyOnce)
        .await?;
    mqtt.subscribe(channels::ONESHOT_OVERRIDE_SET, QoS::ExactlyOnce)
        .await?;
    mqtt.subscribe(channels::HVAC_MODE, QoS::AtLeastOnce)
        .await?;
    mqtt.subscribe(channels::REMOTESTATE, QoS::AtLeastOnce)
        .await?;

    let probes: BTreeMap<String, String> = redis.hgetall(keys::PROBE_ENDPOINTS).await?;
    let probe_lookup: BTreeMap<&str, &str> = probes.iter().map(|(a, b)| (&**b, &**a)).collect();

    for (_, topic) in &probes {
        mqtt.subscribe(topic, QoS::AtLeastOnce).await?;
    }

    loop {
        use rumqttc::{Event, Packet};
        match mqtt_eventloop.poll().await? {
            Event::Incoming(Packet::Publish(message)) => match &*message.topic {
                channels::HVAC_MODE => {
                    if let Some(mode) = HvacRequest::from_payload(&message.payload) {
                        state.mode.set(mode.into());
                    }
                }

                channels::SCRIPT_DATA_GET => {
                    mqtt.publish(
                        channels::SCRIPT_DATA,
                        QoS::ExactlyOnce,
                        true,
                        &*state.script.get().0,
                    )
                    .await?;
                }
                channels::SCRIPT_DATA_SET => {
                    let Ok(script) = std::str::from_utf8(&message.payload) else {
                        mqtt.publish(
                            channels::SCRIPT_DATA_ERROR,
                            QoS::ExactlyOnce,
                            false,
                            serde_json::json!({
                                "success": false,
                                "error": "Utf8Error",
                            })
                            .to_string(),
                        )
                        .await
                        .ok();
                        continue;
                    };

                    println!("Updating script due to incoming topic {}", message.topic);
                    redis.set(keys::SAVED_SCRIPT, script).await?;
                    state.script.set(Arc::new((script.into(), Utc::now())));
                    mqtt.publish(channels::SCRIPT_DATA, QoS::ExactlyOnce, true, script)
                        .await?;
                }
                channels::SCRIPT_DATA_TEST => {
                    let Ok(script) = std::str::from_utf8(&message.payload) else {
                        mqtt.publish(
                            channels::SCRIPT_DATA_TEST_ERROR,
                            QoS::ExactlyOnce,
                            false,
                            serde_json::json!({
                                "success": false,
                                "error": "Utf8Error",
                            })
                            .to_string(),
                        )
                        .await
                        .ok();
                        continue;
                    };

                    let test_state = ScriptState {
                        redis: redis.clone(),
                        mqtt: mqtt.clone(),
                        state: {
                            let mut copy_state = state.clone();
                            Arc::make_mut(&mut copy_state);
                            copy_state
                        },
                    };
                    if let Err(e) = test_script(script, &test_state).await {
                        mqtt.publish(
                            channels::SCRIPT_DATA_TEST_ERROR,
                            QoS::ExactlyOnce,
                            false,
                            serde_json::json!({
                                "success": false,
                                "error": format!("{e:?}"),
                            })
                            .to_string(),
                        )
                        .await
                        .ok();
                    } else {
                        mqtt.publish(
                            channels::SCRIPT_DATA_TEST_ERROR,
                            QoS::ExactlyOnce,
                            false,
                            serde_json::json!({
                                "success": true,
                            })
                            .to_string(),
                        )
                        .await
                        .ok();
                    }
                }

                channels::TIMED_OVERRIDE_GET => {
                    mqtt.publish(
                        channels::TIMED_OVERRIDE,
                        QoS::ExactlyOnce,
                        true,
                        serde_json::to_string(&*state.timed_override.get())?,
                    )
                    .await?;
                }
                channels::TIMED_OVERRIDE_SET => {
                    let new_override =
                        match serde_json::from_slice::<Option<TimedOverride>>(&message.payload) {
                            Ok(to) => to,
                            Err(e) => {
                                mqtt.publish(
                                    channels::TIMED_OVERRIDE_ERROR,
                                    QoS::ExactlyOnce,
                                    false,
                                    serde_json::json!({
                                        "success": false,
                                        "error": format!("{e:?}"),
                                    })
                                    .to_string(),
                                )
                                .await?;
                                continue;
                            }
                        };

                    let normalized_data = serde_json::to_string(&new_override)?;
                    redis.set(keys::TIMED_OVERRIDE, &normalized_data).await?;
                    state.timed_override.set(Arc::new(new_override));
                    publish_timed_override(&mqtt, &state).await?;
                }

                channels::ONESHOT_OVERRIDE_GET => {
                    mqtt.publish(
                        channels::ONESHOT_OVERRIDE,
                        QoS::ExactlyOnce,
                        false,
                        serde_json::to_string(&*state.oneshot_override.get())?,
                    )
                    .await?;
                }
                channels::ONESHOT_OVERRIDE_SET => {
                    let new_override =
                        match serde_json::from_slice::<Option<OneshotOverride>>(&message.payload) {
                            Ok(to) => to,
                            Err(e) => {
                                mqtt.publish(
                                    channels::ONESHOT_OVERRIDE_ERROR,
                                    QoS::ExactlyOnce,
                                    false,
                                    serde_json::json!({
                                        "success": false,
                                        "error": format!("{e:?}"),
                                    })
                                    .to_string(),
                                )
                                .await?;
                                continue;
                            }
                        };

                    let normalized_data = serde_json::to_string(&new_override)?;
                    redis.set(keys::ONESHOT_OVERRIDE, &normalized_data).await?;
                    state.oneshot_override.set(Arc::new(new_override));
                    publish_oneshot_override(&mqtt, &state).await?;
                }

                channels::REMOTESTATE => {
                    if let Some(call) = HvacRequest::from_payload(&message.payload) {
                        state.last_call.set(Arc::new(call));
                    }
                }

                probe_topic if let Some(&probe) = probe_lookup.get(probe_topic) => {
                    let Some(temperature) = std::str::from_utf8(&message.payload)
                        .ok()
                        .and_then(|s| s.parse::<f64>().ok())
                    else {
                        continue;
                    };

                    let mut probe_values = state.probe_values.get();
                    Arc::make_mut(&mut probe_values).insert(probe.into(), temperature);
                    state.probe_values.set(probe_values);
                }

                retained_topic
                    if state
                        .retained_keys
                        .read()
                        .unwrap()
                        .contains_key(retained_topic) =>
                {
                    let Ok(value) = std::str::from_utf8(&message.payload) else {
                        continue;
                    };

                    state
                        .retained_keys
                        .write()
                        .unwrap()
                        .insert(retained_topic.into(), value.into());
                }

                _ => {}
            },
            _ => {}
        }
    }
}

pub async fn publish_timed_override(
    mqtt: &rumqttc::AsyncClient,
    state: &CommonState,
) -> anyhow::Result<()> {
    mqtt.publish(
        channels::TIMED_OVERRIDE,
        QoS::AtLeastOnce,
        true,
        serde_json::to_string(&*state.timed_override.get())?,
    )
    .await?;
    Ok(())
}

pub async fn publish_oneshot_override(
    mqtt: &rumqttc::AsyncClient,
    state: &CommonState,
) -> anyhow::Result<()> {
    mqtt.publish(
        channels::ONESHOT_OVERRIDE,
        QoS::AtLeastOnce,
        true,
        serde_json::to_string(&*state.oneshot_override.get())?,
    )
    .await?;
    Ok(())
}
