use models::timed_rule::TimedRuleSet;
use sycamore::{futures::spawn_local_scoped, prelude::*};
use web_sys::Event;

use crate::helpers::{refresh_signal, create_saved_signal};

mod viewer;

#[component]
pub async fn RulesEditor(cx: Scope<'_>) -> View<DomNode> {
    let current_ruleset = create_saved_signal(cx, "current-ruleset", TimedRuleSet::default());
    spawn_local_scoped(cx, async move {
        refresh_signal("thermostat/rules/current", current_ruleset, |x| x).await;
    });

    let current_open = create_saved_signal(cx, "current-ruleset-open", false);
    let toggle_current = |e: Event| {
        e.prevent_default();
        current_open.set(!*current_open.get());
    };

    view! { cx,
        a(href="#/", class="link-button", on:click=toggle_current) {
            h3 {
                "Current Ruleset"
                (if *current_open.get() {
                    "▼"
                } else {
                    "▶"
                })
            }
        }
        (if *current_open.get() {
            view! { cx,
                viewer::RuleViewer(ruleset = current_ruleset)
            }
        } else {
            view! { cx, }
        })
        hr {}
    }
}
