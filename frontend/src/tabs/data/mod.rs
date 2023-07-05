use sycamore::prelude::*;

#[component]
pub fn DataPage(cx: Scope<'_>) -> View<DomNode> {
    view! { cx,
        h2(class = "page-title") { "Data" }

        crate::controls::thermostat::temp_display::TemperatureDisplay()
        crate::controls::thermostat::history::TemperatureHistory()
    }
}
