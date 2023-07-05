use serde::{Deserialize, Serialize};
use sycamore::prelude::*;
use web_sys::Event;

use crate::helpers::create_saved_signal;

mod admin;
mod data;
mod hvac;
mod quick;

#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
enum ActiveTab {
    Quick,
    Data,
    Hvac,
    Admin,
}

#[derive(Prop, Default)]
pub struct TabRootParams {
    pub is_admin: bool,
}

#[component]
pub fn TabRoot(cx: Scope, params: TabRootParams) -> View<DomNode> {
    let active_tab = create_saved_signal(cx, "active_tab", ActiveTab::Quick);

    if !params.is_admin && *active_tab.get_untracked() == ActiveTab::Admin {
        active_tab.set(ActiveTab::Quick)
    }

    let quick_class = create_selector(cx, || match *active_tab.get() {
        ActiveTab::Quick => "tab-button highlighted",
        _ => "tab-button",
    });
    let data_class = create_selector(cx, || match *active_tab.get() {
        ActiveTab::Data => "tab-button highlighted",
        _ => "tab-button",
    });
    let hvac_class = create_selector(cx, || match *active_tab.get() {
        ActiveTab::Hvac => "tab-button highlighted",
        _ => "tab-button",
    });
    let admin_class = create_selector(cx, || match *active_tab.get() {
        ActiveTab::Admin => "tab-button highlighted",
        _ => "tab-button",
    });

    let quick_click = move |_e: Event| {
        active_tab.set(ActiveTab::Quick)
    };
    let data_click = move |_e: Event| {
        active_tab.set(ActiveTab::Data)
    };
    let hvac_click = move |_e: Event| {
        active_tab.set(ActiveTab::Hvac)
    };
    let admin_click = move |_e: Event| {
        active_tab.set(ActiveTab::Admin)
    };

    view! { cx,
        div(class = "tab-bar") {
            div(class = quick_class, on:click = quick_click) {
                "⚡"
            }
            div(class = data_class, on:click = data_click) {
                "📈"
            }
            div(class = hvac_class, on:click = hvac_click) {
                "📐"
            }
            (if params.is_admin {
                view! { cx,
                    div(class = admin_class, on:click = admin_click) {
                        "🔐"
                    }
                }
            } else {
                view! { cx, }
            })
        }

        div(class = "active-tab") {
            (match *active_tab.get() {
                ActiveTab::Quick => view!{ cx, quick::QuickAccessPage() },
                ActiveTab::Data => view!{ cx, data::DataPage() },
                ActiveTab::Hvac => view!{ cx, hvac::HvacConfigPage() },
                ActiveTab::Admin => view!{ cx, admin::AdminPage() },
            })
        }
    }
}
