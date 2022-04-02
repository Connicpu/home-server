use std::time::Duration;

use gloo_timers::future::sleep;
use reqwest::StatusCode;
use serde::Deserialize;
use sycamore::{futures::ScopeSpawnLocal, prelude::*};
use web_sys::window;

use crate::{
    auth::auth_token,
    controls::thermostat::{HvacRequest, Units},
    helpers::create_saved_signal,
};

#[component]
pub fn TemperatureDisplay(cx: ScopeRef) -> View<DomNode> {
    let units = create_saved_signal(cx, "thermostat-units", Units::Celcius);
    let temperature = cx.create_signal(None::<f32>);
    let pinstate = cx.create_signal(HvacRequest::Off);

    start_refresh_state(cx, temperature, pinstate);

    let temperature_display = cx.create_selector(|| match (*temperature.get(), *units.get()) {
        (Some(temp), Units::Celcius) => format!("{:.1}°C", temp),
        (Some(temp), Units::Fahrenheit) => format!("{:.0}°F", temp * 9. / 5. + 32.),
        (None, _) => format!("..."),
    });

    let temperature_status = cx.create_selector(|| match *pinstate.get() {
        HvacRequest::Off => "thermostat-off",
        HvacRequest::Heat => "thermostat-heat",
        HvacRequest::Cool => "thermostat-cool",
    });

    let toggle_units = move |_| {
        match *units.get() {
            Units::Celcius => units.set(Units::Fahrenheit),
            Units::Fahrenheit => units.set(Units::Celcius),
        }
        cx.spawn_local(async move {
            refresh_state(temperature, pinstate).await;
        });
    };

    view! { cx,
        div(id="thermostat-current-temp-wrapper", class=temperature_status, on:click=toggle_units) {
            span { (temperature_display.get()) }
        }
    }
}

async fn get_temperature(probe: &str) -> Option<f32> {
    let base = window().unwrap().origin();
    let response = reqwest::Client::new()
        .get(format!("{base}/api/thermostat/probes/{probe}/temperature"))
        .header("X-Auth", auth_token())
        .send()
        .await
        .ok()?;

    if response.status() != StatusCode::OK {
        return None;
    }

    let text = response.text().await.ok()?;
    text.parse().ok()
}

async fn get_pinstate() -> Option<HvacRequest> {
    let base = window().unwrap().origin();
    let response = reqwest::Client::new()
        .get(format!(
            "{base}/api/thermostat/pinstate/history?start=0&stop=0"
        ))
        .header("X-Auth", auth_token())
        .send()
        .await
        .ok()?;

    if response.status() != StatusCode::OK {
        return None;
    }

    #[derive(Deserialize)]
    struct HistoryEntry {
        state: HvacRequest,
    }

    let text = response.text().await.ok()?;
    serde_json::from_str::<Vec<HistoryEntry>>(&text)
        .ok()?
        .get(0)
        .map(|e| e.state)
}

fn start_refresh_state<'a>(
    cx: ScopeRef<'a>,
    temperature: &'a Signal<Option<f32>>,
    pinstate: &'a Signal<HvacRequest>,
) {
    cx.spawn_local(async move {
        loop {
            refresh_state(temperature, pinstate).await;

            sleep(Duration::from_secs(3)).await;
        }
    })
}

async fn refresh_state(temperature: &Signal<Option<f32>>, pinstate: &Signal<HvacRequest>) {
    if let Some(temp) = get_temperature("primary").await {
        temperature.set(Some(temp));
    }

    if let Some(ps) = get_pinstate().await {
        pinstate.set(ps);
    }
}
