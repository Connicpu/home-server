use sycamore::prelude::*;

use crate::models::HvacMode;

mod rules;

#[component]
pub fn HvacConfigPage(cx: Scope<'_>) -> View<DomNode> {
    let hvac_mode = use_context::<Signal<HvacMode>>(cx);

    view! { cx,
        h2(class = "page-title") { "Hvac Config" }
        div {
            "Mode: "
            (hvac_mode.get())
        }
        
        hr {}

        rules::RulesEditor()
    }
}
