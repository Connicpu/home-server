#![feature(let_else)]
#![allow(non_snake_case)]

use sycamore::{futures::ScopeSpawnLocal, prelude::*};

use crate::helpers::create_saved_signal;

mod auth;
mod controls;

mod helpers;

fn main() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));

    sycamore::render(|cx| {
        view! { cx,
            App()
        }
    })
}

#[component]
fn App(cx: ScopeRef) -> View<DomNode> {
    let logged_in = cx.create_signal(None);

    // Check our current log-in status first
    cx.spawn_local(async move {
        auth::check_logged_in(logged_in).await;
    });

    view! { cx,
        div(class="main-body") {
            (if *logged_in.get() == Some(false) {
                view! { cx, auth::LoginForm(logged_in) }
            } else if *logged_in.get() == Some(true) {
                view! { cx, Main(logged_in) }
            } else {
                view! { cx, "Please Wait 💕" }
            })
        }
    }
}

#[component]
fn Main<'a>(cx: ScopeRef<'a>, logged_in: &'a Signal<Option<bool>>) -> View<DomNode> {
    let logout = move |_| {
        cx.spawn_local(async move {
            auth::logout(logged_in).await;
        })
    };

    let AtticFan = control_panel("Attic Fan", "attic-fan", controls::AtticFan);
    let Thermostat = control_panel("Thermostat", "thermostat", controls::Thermostat);

    view! { cx,
        a(href="#/", on:click=logout, class="logout") {
            "Logout"
        }

        AtticFan()
        Thermostat()
        (if *logged_in.get() == Some(true) && auth::is_auth_level(3) {
            let Admin = control_panel("Admin", "admin", controls::Admin);
            view! { cx, Admin() }
        } else {
            view! { cx, }
        })
    }
}

fn control_panel<G: Html>(
    title: &'static str,
    pref_key: &'static str,
    Inner: fn(ScopeRef, ()) -> View<G>,
) -> impl Fn(ScopeRef, ()) -> View<G> {
    move |cx: ScopeRef, _| {
        let visible = create_saved_signal(cx, pref_key, false);
        let toggle = |_| {
            visible.set(!*visible.get());
        };
        let display_class = cx.create_selector(|| match *visible.get() {
            true => "",
            false => "collapsed",
        });
        view! { cx,
            div(class="control-panel") {
                a(href="#/", on:click=toggle, class="link-button") {
                    h1 { (title) }
                }
                div(class=display_class) {
                    Inner()
                }
            }
            hr
        }
    }
}
