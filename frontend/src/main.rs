#![allow(non_snake_case)]
#![feature(iter_array_chunks)]

use std::time::Duration;
use sycamore::{futures::spawn_local_scoped, prelude::*};

use crate::helpers::{create_saved_signal, start_signal_refresher};
use crate::models::{HvacMode, HvacModeState, HvacRequest, PinState, Temperature, Units};

mod ace;
mod auth;
mod controls;
mod tabs;

mod helpers;
mod models;

#[derive(Copy, Clone, PartialEq, Eq, Default)]
pub struct LoggedInState {
    logged_in: Option<bool>,
}

fn main() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));

    sycamore::render(|cx| {
        // Global context signals
        let units = create_saved_signal(cx, "thermostat-units", Units::Celcius);
        provide_context_ref(cx, units);

        let logged_in = create_signal(cx, LoggedInState::default());
        provide_context_ref(cx, logged_in);

        // Check our current log-in status first
        spawn_local_scoped(cx, async move {
            auth::check_logged_in(logged_in).await;
        });
    
        let hvac_mode = create_saved_signal(cx, "cached-hvac-mode", HvacMode::Off);
        provide_context_ref(cx, hvac_mode);
        start_signal_refresher(
            cx,
            "thermostat/mode",
            hvac_mode,
            Duration::from_secs(30),
            |ms: HvacModeState| ms.mode,
        );
    
        let temperature = create_saved_signal(cx, "cached-temperature", None::<Temperature>);
        provide_context_ref(cx, temperature);
        start_signal_refresher(
            cx,
            "thermostat/probes/primary/temperature",
            temperature,
            Duration::from_secs(3),
            |x| Some(Temperature(x)),
        );
    
        let pinstate = create_saved_signal(cx, "cached-pinstate", PinState(HvacRequest::Off));
        provide_context_ref(cx, pinstate);
        {
            #[derive(serde::Deserialize, serde::Serialize)]
            struct HistoryEntry {
                state: HvacRequest,
            }
            start_signal_refresher(
                cx,
                "thermostat/pinstate/history?start=0&stop=0",
                pinstate,
                Duration::from_secs(3),
                |x: Vec<HistoryEntry>| PinState(x.get(0).map(|e| e.state).unwrap_or(HvacRequest::Off)),
            );
        }

        view! { cx,
            App()
        }
    })
}

#[component]
fn App(cx: Scope) -> View<DomNode> {
    let logged_in = use_context::<Signal<LoggedInState>>(cx);

    view! { cx,
        div(class="main-body") {
            (if logged_in.get().logged_in == Some(false) {
                view! { cx, auth::LoginForm(logged_in) }
            } else if logged_in.get().logged_in == Some(true) {
                view! { cx, Main(logged_in) }
            } else {
                view! { cx, "Please Wait 💕" }
            })
        }
    }
}

#[component]
fn Main<'a>(cx: Scope<'a>, logged_in: &'a Signal<LoggedInState>) -> View<DomNode> {
    let logout = move |_| {
        spawn_local_scoped(cx, async move {
            auth::logout(logged_in).await;
        })
    };

    view! { cx,
        a(href="#/", on:click=logout, class="logout") {
            "Logout"
        }

        tabs::TabRoot(is_admin = logged_in.get().logged_in == Some(true) && auth::is_auth_level(3))
    }
}
