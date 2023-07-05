use sycamore::prelude::*;

use crate::models::{HvacRequest, PinState, Temperature, Units};

#[component]
pub fn TemperatureDisplay(cx: Scope) -> View<DomNode> {
    let units = use_context::<Signal<Units>>(cx);
    let temperature = use_context::<Signal<Option<Temperature>>>(cx);
    let pinstate = use_context::<Signal<PinState>>(cx);

    let temperature_display = create_selector(cx, || {
        match (temperature.get().map(|t| t.0), *units.get()) {
            (Some(temp), Units::Celcius) => format!("{:.2}°C", temp),
            (Some(temp), Units::Fahrenheit) => format!("{:.1}°F", temp * 9. / 5. + 32.),
            (None, _) => format!("..."),
        }
    });

    let temperature_status = create_selector(cx, || match pinstate.get().0 {
        HvacRequest::Off => "thermostat-off",
        HvacRequest::Heat => "thermostat-heat",
        HvacRequest::Cool => "thermostat-cool",
    });

    let toggle_units = move |_| match *units.get() {
        Units::Celcius => units.set(Units::Fahrenheit),
        Units::Fahrenheit => units.set(Units::Celcius),
    };

    view! { cx,
        div(id="thermostat-current-temp-wrapper", class=temperature_status, on:click=toggle_units) {
            span { (temperature_display.get()) }
        }
    }
}
