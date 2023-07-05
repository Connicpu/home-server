use sycamore::prelude::*;

use crate::controls::{AtticFan, thermostat::{temp_display::TemperatureDisplay, cmd_override::CommandOverride, oneshot_setpoint::OneshotSetpoint}};

#[component]
pub fn QuickAccessPage(cx: Scope<'_>) -> View<DomNode> {
    view! { cx,
        h2(class = "page-title") { "Quick Access" }

        TemperatureDisplay()

        hr {}

        AtticFan()

        hr {}

        // TODO: Replace this
        CommandOverride()

        hr {}

        OneshotSetpoint()
    }
}
