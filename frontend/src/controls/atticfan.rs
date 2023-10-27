use std::{rc::Rc, time::Duration};

use gloo_timers::future::sleep;
use reqwest::StatusCode;
use sycamore::{prelude::*, futures::spawn_local_scoped};
use web_sys::window;

use crate::auth::auth_token;

#[component]
pub fn AtticFan(cx: Scope) -> View<DomNode> {
    let big_succ_state = create_signal(cx, false);
    let roof_fan_state = create_signal(cx, false);

    start_refresh_state_loop(cx, big_succ_state, roof_fan_state);

    let big_succ_class = create_selector(cx, || indicator_class(big_succ_state.get()));
    let roof_fan_class = create_selector(cx, || indicator_class(roof_fan_state.get()));

    let big_succ_value = create_selector(cx, || indicator_value(big_succ_state.get()));
    let roof_fan_value = create_selector(cx, || indicator_value(roof_fan_state.get()));

    let toggle_big_succ = move |_| {
        let new_state = !*big_succ_state.get();
        big_succ_state.set(new_state);
        spawn_local_scoped(cx, async move {
            set_state(BIG_SUCC, new_state).await;
        });
    };

    let toggle_roof_fan = move |_| {
        let new_state = !*roof_fan_state.get();
        roof_fan_state.set(new_state);
        spawn_local_scoped(cx, async move {
            set_state(ROOF_FAN, new_state).await;
        });
    };

    view! { cx,
        table(id="atticfan-control") {
            tr {
                td { "Big Succ" }
                td { "Roof Fan" }
            }
            tr {
                td {
                    a(href="#/", on:click=toggle_big_succ, class="link-button") {
                        div(class=big_succ_class) {
                            (big_succ_value.get())
                        }
                    }
                }
                td {
                    a(href="#/", on:click=toggle_roof_fan, class="link-button") {
                        div(class=roof_fan_class) {
                            (roof_fan_value.get())
                        }
                    }
                }
            }
        }
    }
}

fn indicator_class(state: Rc<bool>) -> &'static str {
    if *state {
        "status-on"
    } else {
        "status-off"
    }
}

fn indicator_value(state: Rc<bool>) -> &'static str {
    if *state {
        "ON"
    } else {
        "OFF"
    }
}

const BIG_SUCC: i32 = 1;
const ROOF_FAN: i32 = 0;

async fn get_state(fan: i32) -> bool {
    let base = window().unwrap().origin();
    let Ok(response) = reqwest::Client::new()
        .get(format!("{base}/api/atticfan/getstate/{fan}"))
        .header("X-Auth", auth_token())
        .send()
        .await else {
            return false;
        };

    if response.status() == StatusCode::OK {
        if let Ok(text) = response.text().await {
            return text == "true";
        }
    }
    false
}

async fn set_state(fan: i32, state: bool) {
    let base = window().unwrap().origin();
    let _ = reqwest::Client::new()
        .get(format!("{base}/api/atticfan/setstate/{fan}/{state}"))
        .header("X-Auth", auth_token())
        .send()
        .await;
}

fn start_refresh_state_loop<'a>(cx: Scope<'a>, bs: &'a Signal<bool>, rf: &'a Signal<bool>) {
    spawn_local_scoped(cx, async move {
        loop {
            bs.set(get_state(BIG_SUCC).await);
            rf.set(get_state(ROOF_FAN).await);
            sleep(Duration::from_secs(10)).await;
        }
    })
}
