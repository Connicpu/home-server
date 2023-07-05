use std::time::Duration;
use std::cmp;

use sycamore::{futures::spawn_local_scoped, prelude::*};
use web_sys::{window, Event};

use crate::{
    auth::auth_token,
    helpers::{create_saved_signal, refresh_signal, start_signal_refresher},
    models::{HvacMode, HvacRequest, OneshotOrdering, OneshotSetpointState, Temperature, Units},
};

const ENDPOINT: &str = "thermostat/oneshot_setpoint";

#[component]
pub fn OneshotSetpoint(cx: Scope<'_>) -> View<DomNode> {
    let component_open = create_saved_signal(cx, "oneshot-setpoint-component-open", false);
    let panel_open = create_signal(cx, false);
    let hvac_mode = use_context::<Signal<HvacMode>>(cx);
    let units = use_context::<Signal<Units>>(cx);
    let temperature = use_context::<Signal<Option<Temperature>>>(cx);

    let current_program = create_signal(cx, None::<OneshotSetpointState>);
    start_signal_refresher(
        cx,
        ENDPOINT,
        current_program,
        Duration::from_secs(30),
        |x| x,
    );

    let selected_cmd = create_saved_signal(cx, "oneshot_cmd_selection", "off".to_string());
    let selected_setpoint =
        create_saved_signal(cx, "oneshot_setpoint_selection", "21.0".to_string());

    let units_display = create_selector(cx, || match *units.get() {
        Units::Celcius => "°C",
        Units::Fahrenheit => "°F",
    });

    let component_class = create_selector(cx, || match *component_open.get() {
        true => "",
        false => "collapsed",
    });
    let send_text = create_selector(cx, || match *panel_open.get() {
        true => "Cancel",
        false => "Send Command",
    });
    let panel_class = create_selector(cx, || match *panel_open.get() {
        true => "thermostat-cmd-panel",
        false => "collapsed",
    });

    let toggle_display = move |e: Event| {
        e.prevent_default();
        component_open.set(!*component_open.get());
    };

    let toggle_command = move |e: Event| {
        e.prevent_default();
        panel_open.set(!*panel_open.get());
    };

    let send_cmd = move |e: Event| {
        e.prevent_default();
        let selected_cmd = match &**selected_cmd.get().clone() {
            "off" => HvacRequest::Off,
            "heat" => HvacRequest::Heat,
            "cool" => HvacRequest::Cool,
            _ => return,
        };

        let Ok(setpoint) = selected_setpoint.get().clone().parse::<f32>() else {
                return;
            };

        let Some(temp) = *temperature.get() else { return };
        let comparison = match (selected_cmd, setpoint.partial_cmp(&temp.0)) {
            (HvacRequest::Cool, _) => OneshotOrdering::Less,
            (HvacRequest::Heat, _) => OneshotOrdering::Greater,
            (HvacRequest::Off, Some(cmp::Ordering::Less)) => OneshotOrdering::Less,
            (HvacRequest::Off, Some(cmp::Ordering::Greater)) => OneshotOrdering::Greater,
            _ => OneshotOrdering::Less,
        };

        let request = OneshotSetpointState {
            action: selected_cmd,
            setpoint,
            comparison,
        };

        spawn_local_scoped(cx, async move {
            send_command(Some(request)).await;
            refresh_signal(ENDPOINT, current_program, |x| x).await;
            panel_open.set(false);
        })
    };

    let send_cancel = move |e: Event| {
        e.prevent_default();
        spawn_local_scoped(cx, async move {
            send_command(None).await;
            refresh_signal(ENDPOINT, current_program, |x| x).await;
            panel_open.set(false);
        });
    };

    view! { cx,
        a(href="#/", class="link-button", on:click=toggle_display) {
            h2 {
                "Run To Setpoint"
                (if *component_open.get() {
                    "▼"
                } else {
                    "▶"
                })
            }
        }
        div(class=component_class) {
            (if let Some(state) = *current_program.get() {
                view! { cx,
                    div(style="font-size:1.5em;margin-bottom:0.5em") {
                        "Current Program: "
                        (state.action)
                        " until "
                        (state.setpoint)
                        (units_display.get())
                    }
                }
            } else {
                view! { cx,
                    div(style="font-size:1.5em;margin-bottom:0.5em") {
                        "No Program Set"
                    }
                }
            })
            (if *hvac_mode.get() != HvacMode::Off {
                view! { cx,
                    a(href="#", class="link-button", on:click=toggle_command) {
                        span(class="link-button-bg") {
                            (send_text.get())
                        }
                    }
                    div(class=panel_class) {
                        div(style="margin-bottom: 0.5em") {
                            select(bind:value=selected_cmd) {
                                option(value="off", selected=*selected_cmd.get()=="off") { "Off" }
                                option(value="heat", selected=*selected_cmd.get()=="heat") { "Heat" }
                                option(value="cool", selected=*selected_cmd.get()=="cool") { "Cool" }
                            }
                            " until "
                            (if *units.get() == Units::Celcius {
                                view! { cx,
                                    input(bind:value=selected_setpoint, type="range", min=18.0, max=25.0, step="0.25", style="margin-top: 0.25em")
                                    " "
                                    (format!("{selected_setpoint}°C"))
                                }
                            } else {
                                view! { cx,
                                    input(bind:value=selected_setpoint, type="range", min=65.0, max=77.0, step="0.5", style="margin-top: 0.25em")
                                    " "
                                    (format!("{selected_setpoint}°F"))
                                }
                            })
                        }
                        div(class="thermostat-cmd-entry-item") {
                            a(href="#/", class="link-button", on:click=send_cmd) {
                                span(class="link-button-bg") {
                                    "Send"
                                }
                            }
                            a(href="#/", class="link-button", on:click=send_cancel) {
                                span(class="link-button-bg") {
                                    "Clear Command"
                                }
                            }
                        }
                    }
                }
            } else {
                view! { cx,
                    "Thermostat Mode is set to Off"
                }
            })
        }
    }
}

async fn send_command(cmd: Option<OneshotSetpointState>) {
    let base = window().unwrap().origin();
    reqwest::Client::new()
        .put(format!("{base}/api/thermostat/oneshot_setpoint"))
        .header("X-Auth", auth_token())
        .body(serde_json::to_string(&cmd).unwrap())
        .send()
        .await
        .ok();
}
