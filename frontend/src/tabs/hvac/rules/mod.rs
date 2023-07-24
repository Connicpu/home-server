use std::{collections::BTreeSet, time::Duration};

use gloo_timers::future::sleep;
use gloo_utils::format::JsValueSerdeExt;
use models::hvac_request::HvacRequest;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sycamore::{futures::spawn_local_scoped, prelude::*};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{window, Event, HtmlElement};

use crate::{
    ace::{self, Editor},
    auth::auth_token,
    helpers::{create_saved_signal, refresh_signal},
};

#[component]
pub async fn RulesEditor(cx: Scope<'_>) -> View<DomNode> {
    let lua_title = create_saved_signal(cx, "lua-editor-script-title", "configname".to_string());
    let lua_text = create_saved_signal(cx, "lua-editor-text", SAMPLE_LUA_CONFIG.to_string());
    let lua_edit_ref = create_node_ref(cx);
    let editor_ref = create_signal(cx, None);
    on_mount(cx, move || {
        let edit_node = lua_edit_ref.get::<DomNode>();
        let edit_elem = edit_node.inner_element().dyn_into::<HtmlElement>().unwrap();

        let editor = ace::edit(
            edit_elem,
            <JsValue as JsValueSerdeExt>::from_serde(&serde_json::json!({
                "mode": "ace/mode/lua",
                "selectionStyle": "text",
                "useWorker": true,
                "enableBasicAutocompletion": true,
                "enableLiveAutocompletion": true,
            }))
            .unwrap(),
        );
        editor_ref.set(Some(editor.clone()));

        editor.set_value(&*lua_text.get());
        editor.selection().clear_selection();

        spawn_local_scoped(cx, async move {
            let mut last_text = editor.get_value();
            loop {
                sleep(Duration::from_secs(1)).await;
                let new_text = editor.get_value();
                if new_text != last_text {
                    lua_text.set(new_text.clone());
                    last_text = new_text;
                }
            }
        });
    });

    let script_list = create_signal(cx, Vec::new());
    spawn_local_scoped(cx, async move {
        refresh_signal("thermostat/lua/scripts", script_list, |x: Vec<String>| x).await
    });

    let save_error = create_signal(cx, String::new());
    let do_save = {
        move |_e: Event| {
            let window = window().unwrap();
            let name = lua_title.get();
            if name.is_empty() {
                save_error.set("Empty name not allowed".into());
                return;
            }

            if script_list.get().contains(&*name) {
                if !window
                    .confirm_with_message(&format!(
                        "Are you sure you want to overwrite {name} with the new script?"
                    ))
                    .unwrap()
                {
                    return;
                }
            }

            let editor = editor_ref.get();
            let Some(editor) = (*editor).clone() else { return };
            let script = editor.get_value();
            spawn_local_scoped(cx, async move {
                save_script(&name, script, save_error).await;
                refresh_signal("thermostat/lua/scripts", script_list, |x: Vec<String>| x).await;
            });
        }
    };

    let validation_results = create_signal(cx, String::new());
    let validation_success = create_signal(cx, false);
    let activate_btn_class = create_selector(cx, move || match *validation_success.get() {
        true => "",
        false => "collapsed",
    });
    let do_validate = {
        move |_e: Event| {
            let editor = editor_ref.get();
            let Some(editor) = (*editor).clone() else { return };
            let script_text = editor.get_value();
            spawn_local_scoped(cx, async move {
                validate_script(script_text, validation_results, validation_success).await;
            });
        }
    };

    let do_activate = {
        move |_e: Event| {
            let editor = editor_ref.get();
            let Some(editor) = (*editor).clone() else { return };
            let script_text = editor.get_value();
            spawn_local_scoped(cx, async move {
                activate_script(script_text, validation_results).await;
            })
        }
    };

    let do_load_active = {
        move |_e: Event| {
            let editor = editor_ref.get();
            let Some(editor) = (*editor).clone() else { return };
            spawn_local_scoped(cx, async move {
                load_active_script(editor).await;
            })
        }
    };

    let issues = create_signal(cx, String::new());
    let get_issues = move |_e: Event| {
        spawn_local_scoped(cx, async move {
            refresh_signal(
                "thermostat/lua/issues",
                issues,
                move |issues: Vec<String>| {
                    format!("{} issues\n{}", issues.len(), issues.join("\n"))
                },
            )
            .await;
        })
    };

    view! { cx,
        h3 { "Saved Scripts" }
        table {
            Indexed(
                iterable=script_list,
                view=move |cx, name| {
                    let do_load = {
                        let name = name.clone();
                        move |_e: Event| {
                            let window = window().unwrap();
                            let editor = editor_ref.get();
                            let Some(editor) = (*editor).clone() else { return };

                            if !window.confirm_with_message(&format!("Overwrite current script with {name}?")).unwrap() {
                                return;
                            }

                            let name = name.clone();
                            spawn_local_scoped(cx, async move {
                                load_script(editor, &name).await;
                                lua_title.set(name);
                            })
                        }
                    };
                    view! { cx,
                        tr {
                            td { (name) }
                            td {
                                input(type="button", value="Load", on:click=do_load)
                            }
                        }
                    }
                }
            )
        }

        hr()

        div {
            label {
                "Script Name "
            }
            input(bind:value=lua_title)
            input(type="button", value="Save", on:click=do_save)
            span(style="color:red") {
                (save_error.get())
            }
        }
        div(ref=lua_edit_ref, style="width:100%;min-height:35em;margin-top:1em") {}
        div {
            input(type="button", value="Load Active Script", on:click=do_load_active)
            input(type="button", value="Validate", on:click=do_validate)
            input(type="button", value="Activate", class=activate_btn_class, on:click=do_activate)
        }
        div {
            pre { (validation_results.get()) }
        }

        hr()

        div {
            input(type="button", value="Get Issues", on:click=get_issues)
        }
        div {
            pre { (issues.get()) }
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct ScriptBody {
    script: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
enum ValidationResponse {
    Error(String),
    Results {
        output: Option<HvacRequest>,
        issues: BTreeSet<String>,
    },
}

async fn load_script(editor: Editor, name: &str) {
    let window = window().unwrap();
    let base = window.origin();
    let result = reqwest::Client::new()
        .get(format!("{base}/api/thermostat/lua/scripts/{name}"))
        .header("X-Auth", auth_token())
        .send()
        .await;

    let Ok(response) = result else { return };
    let Ok(body) = response.json::<ScriptBody>().await else { return };

    editor.set_value(&body.script);
    editor.selection().clear_selection();
}

async fn save_script(name: &str, script: String, error: &Signal<String>) {
    if name.is_empty() {
        return;
    }

    let data = ScriptBody { script };

    let window = window().unwrap();
    let base = window.origin();
    let result = reqwest::Client::new()
        .put(format!("{base}/api/thermostat/lua/scripts/{name}"))
        .header("X-Auth", auth_token())
        .body(serde_json::to_string(&data).unwrap())
        .send()
        .await;

    let response = match result {
        Ok(response) => response,
        Err(e) => {
            error.set(format!("Server Error: {e}"));
            return;
        }
    };

    if response.status() != StatusCode::OK {
        let status = response.status();
        let message = response.text().await.unwrap_or_default();
        error.set(format!("HTTP {status}: {message}"));
        return;
    }

    error.set(String::new());
}

async fn load_active_script(editor: Editor) {
    let window = window().unwrap();
    if !window
        .confirm_with_message("Are you sure you want to overwrite your current script?")
        .unwrap()
    {
        return;
    }

    let base = window.origin();
    let result = reqwest::Client::new()
        .get(format!("{base}/api/thermostat/lua/active_script"))
        .header("X-Auth", auth_token())
        .send()
        .await;

    let Ok(response) = result else { return };
    let Ok(data) = response.json::<ScriptBody>().await else { return };

    editor.set_value(&data.script);
    editor.selection().clear_selection();
}

async fn validate_script(script: String, results: &Signal<String>, is_good: &Signal<bool>) {
    is_good.set(false);

    let window = window().unwrap();
    let base = window.origin();

    let data = ScriptBody { script };

    let result = reqwest::Client::new()
        .post(format!("{base}/api/thermostat/lua/validate"))
        .header("X-Auth", auth_token())
        .body(serde_json::to_string(&data).unwrap())
        .send()
        .await;

    let response = match result {
        Ok(response) => response,
        Err(e) => {
            results.set(format!("Server Error\n{e}"));
            return;
        }
    };

    if response.status() != StatusCode::OK {
        let status = response.status();
        let message = response.text().await.unwrap_or_default();
        results.set(format!("HTTP {status}\n{message}"));
        return;
    }

    let output: ValidationResponse = match response.json().await {
        Ok(output) => output,
        Err(e) => {
            results.set(format!("Response Error\n{e}"));
            return;
        }
    };

    let mut message = "Validation Results\n".to_string();
    match output {
        ValidationResponse::Error(error) => {
            message += "Validation Error\n";
            message += &error;
        }
        ValidationResponse::Results { output, issues } => {
            message += "Output: ";
            match output {
                Some(output) => message += output.payload_str(),
                None => message += "nil",
            }
            if !issues.is_empty() {
                message += &format!("\n\n{} Issues Found", issues.len());
                for issue in issues {
                    message += "\n";
                    message += &issue;
                }
            } else {
                is_good.set(true);
            }
        }
    }

    results.set(message);
}

async fn activate_script(script: String, results: &Signal<String>) {
    let window = window().unwrap();
    let base = window.origin();

    let data = ScriptBody { script };

    let result = reqwest::Client::new()
        .put(format!("{base}/api/thermostat/lua/active_script"))
        .header("X-Auth", auth_token())
        .body(serde_json::to_string(&data).unwrap())
        .send()
        .await;

    let response = match result {
        Ok(response) => response,
        Err(e) => {
            results.set(format!("Server Error\n{e}"));
            return;
        }
    };

    if response.status() != StatusCode::OK {
        let status = response.status();
        let message = response.text().await.unwrap_or_default();
        results.set(format!("HTTP {status}\n{message}"));
        return;
    }

    results.set(format!("Script activated!"));
}

const SAMPLE_LUA_CONFIG: &str = "function evaluate(state)
    local temp = state.probes.primary.temperature
    return state:timed_program {
        ['22:00'] = function() -- Cool off for bed
            if temp > 21.5 then
                return 'cool'
            elseif temp < 20.8 then
                return 'off'
            end
        end,
        ['05:00'] = function() -- Normal daily operation
            if temp > 23.5 then
                return 'cool'
            elseif temp < 22.5 then
                return 'off'
            end
        end
    }
end
";
