use std::rc::Rc;

use sycamore::{prelude::*, futures::spawn_local_scoped};
use web_sys::window;

#[component]
pub fn Admin(cx: Scope) -> View<DomNode> {
    let grant_username = create_signal(cx, String::new());
    let grant_level = create_signal(cx, "0".to_string());

    let grant_auth = move |_| {
        let username = grant_username.get();
        let Ok(level) = grant_level.get().parse::<i32>() else {
            return;
        };
        if !window()
            .unwrap()
            .confirm_with_message(&format!(
                r#"Are you sure you want to set "{username}" to auth level {level}?"#
            ))
            .unwrap()
        {
            return;
        }

        spawn_local_scoped(cx, async move {
            if let Err(err) = grant_authorization(username, level).await {
                window()
                    .unwrap()
                    .alert_with_message(&format!("Problem granting authroization: {err:#?}"))
                    .unwrap()
            }

            grant_username.set(String::new());
        });
    };

    view! { cx,
        h2 { "Grant Authorization" }
        div {
            input(bind:value=grant_username, placeholder="Username...")
            input(bind:value=grant_level, type="number", min="-1", max="3")
            a(href="#/", class="link-button", on:click=grant_auth) {
                span(class="link-button-bg") {
                    "Grant"
                }
            }
        }
    }
}

async fn grant_authorization(_username: Rc<String>, _level: i32) -> anyhow::Result<()> {
    Ok(())
}
