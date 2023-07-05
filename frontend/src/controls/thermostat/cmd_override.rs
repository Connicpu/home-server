use chrono::{DateTime, Duration, Local, Utc};
use gloo_timers::future::sleep;
use serde::{Deserialize, Serialize};
use sycamore::{prelude::*, futures::spawn_local_scoped};
use web_sys::{window, Event};

use crate::{auth::auth_token, helpers::create_saved_signal, models::HvacRequest};

#[component]
pub fn CommandOverride(cx: Scope) -> View<DomNode> {
    let component_open = create_saved_signal(cx, "command-override-open", false);
    let panel_open = create_signal(cx, false);
    let status = create_signal(cx, "None".to_string());

    refresh_status_loop(cx, status);

    let selected_cmd = create_saved_signal(cx, "override_cmd_selection", "off".to_string());
    let selected_time = create_signal(cx, "10".to_string());

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

        let Ok(selected_time) = selected_time.get().clone().parse::<i64>() else {
                return;
            };

        let request = OverridePulseState {
            active_until: Utc::now() + Duration::minutes(selected_time),
            request: selected_cmd,
        };

        spawn_local_scoped(cx, async move {
            send_command(Some(request)).await;
            refresh_status(status).await;
            panel_open.set(false);
        });
    };

    let send_cancel = move |e: Event| {
        e.prevent_default();
        spawn_local_scoped(cx, async move {
            send_command(None).await;
            refresh_status(status).await;
            panel_open.set(false);
        });
    };

    view! { cx,
        div {
            a(href="#/", class="link-button", on:click=toggle_display) {
                h2 {
                    "Command Override"
                    (if *component_open.get() {
                        "▼"
                    } else {
                        "▶"
                    })
                }
            }
            div(class=component_class) {
                div(style="font-size:1.5em", class="thermostat-cmd-entry-item") {
                    "Current Override: "
                    (status.get())
                }
                a(href="#", class="link-button", on:click=toggle_command) {
                    span(class="link-button-bg") {
                        (send_text.get())
                    }
                }
                div(class=panel_class) {
                    div(class="thermostat-cmd-entry-item") {
                        label {
                            "Command: "
                            select(bind:value=selected_cmd) {
                                option(value="off", selected=*selected_cmd.get()=="off") { "Off" }
                                option(value="heat", selected=*selected_cmd.get()=="heat") { "Heat" }
                                option(value="cool", selected=*selected_cmd.get()=="cool") { "Cool" }
                            }
                        }
                    }
                    div(class="thermostat-cmd-entry-item") {
                        label {
                            "Duration: "
                            input(bind:value=selected_time, type="range", min=2, max=30)
                            " "
                            span(style="white-space: nowrap") {
                                (format!("{selected_time} minutes"))
                            }
                        }
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
        }
    }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct OverridePulseState {
    active_until: DateTime<Utc>,
    request: HvacRequest,
}

async fn send_command(cmd: Option<OverridePulseState>) {
    let base = window().unwrap().origin();
    reqwest::Client::new()
        .put(format!("{base}/api/thermostat/pulse_override"))
        .header("X-Auth", auth_token())
        .body(serde_json::to_string(&cmd).unwrap())
        .send()
        .await
        .ok();
}

async fn refresh_status(status: &Signal<String>) {
    let base = window().unwrap().origin();
    let Ok(response) = reqwest::Client::new()
        .get(format!("{base}/api/thermostat/pulse_override"))
        .header("X-Auth", auth_token())
        .send()
        .await else {
            return;
        };

    let Ok(data) = response.json::<Option<OverridePulseState>>().await else {
        return;
    };

    let Some(data) = data else {
        status.set("None".to_string());
        return;
    };

    if data.active_until < Utc::now() {
        status.set("None".to_string());
    } else {
        let local_time = data.active_until.with_timezone(&Local).time();

        status.set(format!(
            "{:?} until {}",
            data.request,
            local_time.format("%H:%M:%S")
        ));
    }
}

fn refresh_status_loop<'a>(cx: Scope<'a>, status: &'a Signal<String>) {
    spawn_local_scoped(cx, async move {
        loop {
            refresh_status(&status).await;

            sleep(std::time::Duration::from_secs(30)).await;
        }
    });
}
