use std::time::Duration;

use gloo_timers::future::sleep;
use reqwest::StatusCode;
use serde::{de::DeserializeOwned, Serialize};
use sycamore::{futures::spawn_local_scoped, prelude::*};
use web_sys::window;

use crate::auth::auth_token;

pub fn create_saved_signal<'a, T>(cx: Scope<'a>, name: &'static str, default: T) -> &'a Signal<T>
where
    T: Serialize + DeserializeOwned,
{
    fn get_value<T: DeserializeOwned>(name: &str) -> Option<T> {
        let ls = window().unwrap().local_storage().unwrap()?;
        let json = ls.get_item(&format!("saved-signal_{name}")).unwrap()?;
        serde_json::from_str(&json).ok()
    }

    fn set_value<T: Serialize>(name: &str, value: &T) {
        let Some(ls) = window().unwrap().local_storage().unwrap() else { return };
        let json = serde_json::to_string(&value).unwrap();
        ls.set_item(&format!("saved-signal_{name}"), &json).unwrap();
    }

    let initial = get_value(name).unwrap_or(default);

    let signal = create_signal(cx, initial);
    create_effect(cx, move || {
        set_value(name, &*signal.get());
    });

    signal
}

pub async fn refresh_signal<'a, T, J, F>(path: &'static str, signal: &'a Signal<T>, func: F)
where
    J: serde::de::DeserializeOwned,
    F: Fn(J) -> T,
{
    let base = window().unwrap().origin();
    let Ok(response) = reqwest::Client::new()
        .get(format!("{base}/api/{path}"))
        .header("X-Auth", auth_token())
        .send()
        .await else {
            web_sys::console::log_1(&format!("F (reqwest err) ({path})").into());
            return;
        };

    if response.status() != StatusCode::OK {
        web_sys::console::log_1(&format!("F (status code) ({path})").into());
        return;
    }

    let Ok(value) = response.json::<J>().await else {
        web_sys::console::log_1(&format!("F (json fail) ({path})").into());
        return;
    };

    let value = func(value);
    signal.set(value);
}

pub fn start_signal_refresher<'a, T, J, F>(
    cx: Scope<'a>,
    path: &'static str,
    signal: &'a Signal<T>,
    interval: Duration,
    func: F,
) where
    J: serde::de::DeserializeOwned,
    F: Fn(J) -> T + 'a,
{
    spawn_local_scoped(cx, async move {
        loop {
            refresh_signal(path, signal, &func).await;

            sleep(interval).await;
        }
    });
}
