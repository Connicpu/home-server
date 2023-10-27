use anyhow::bail;
use reqwest::StatusCode;
use sycamore::{futures::spawn_local_scoped, prelude::*};
use web_sys::{window, Event};

use crate::{
    auth::auth_token,
    models::{HvacMode, HvacModeState},
};

#[component]
pub fn HvacMode(cx: Scope<'_>) -> View<DomNode> {
    let hvac_mode = use_context::<Signal<HvacMode>>(cx);

    let new_mode_sig = create_signal(cx, String::new());
    create_effect(cx, || {
        new_mode_sig.set(hvac_mode.to_string());
    });

    let submit_mode = move |_e: Event| {
        let new_mode = new_mode_sig
            .get()
            .parse::<HvacMode>()
            .expect("This will only fail if someone fucked with the page");

        spawn_local_scoped(cx, async move {
            if let Err(err) = change_mode(new_mode).await {
                window()
                    .unwrap()
                    .alert_with_message(&format!("{err}"))
                    .unwrap();
                return;
            }

            hvac_mode.set(new_mode);
        });
    };

    view! { cx,
        div {
            "Current Mode: "
            (hvac_mode.get())
        }
        div {
            label {
                "Change Mode: "
                select(bind:value=new_mode_sig) {
                    option(value="Off", selected=*new_mode_sig.get()=="Off") { "Off" }
                    option(value="Heat", selected=*new_mode_sig.get()=="Heat") { "Heat" }
                    option(value="Cool", selected=*new_mode_sig.get()=="Cool") { "Cool" }
                }
            }
            input(type="button", value="Confirm", on:click=submit_mode)
        }
    }
}

async fn change_mode(new_mode: HvacMode) -> anyhow::Result<()> {
    let window = window().unwrap();
    let base = window.origin();
    let result = reqwest::Client::new()
        .put(format!("{base}/api/thermostat/mode"))
        .header("X-Auth", auth_token())
        .body(serde_json::to_string(&HvacModeState { mode: new_mode }).unwrap())
        .send()
        .await?;

    if result.status() != StatusCode::OK {
        bail!("Failed to set HVAC Mode");
    }

    Ok(())
}
