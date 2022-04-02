use serde::{Deserialize, Serialize};
use sycamore::prelude::*;

use self::{cmd_override::CommandOverride, temp_display::TemperatureDisplay, history::TemperatureHistory};

mod cmd_override;
mod history;
mod temp_display;

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum Units {
    Celcius,
    Fahrenheit,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HvacRequest {
    Off,
    Heat,
    Cool,
}

#[component]
pub fn Thermostat(cx: ScopeRef) -> View<DomNode> {
    view! { cx,
        TemperatureDisplay()
        TemperatureHistory()
        CommandOverride()
    }
}
