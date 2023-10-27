use sycamore::prelude::*;

mod mode;
mod rules;

#[component]
pub fn HvacConfigPage(cx: Scope<'_>) -> View<DomNode> {
    view! { cx,
        h2(class = "page-title") { "Hvac Config" }
        mode::HvacMode()
        
        hr {}

        rules::RulesEditor()
    }
}
