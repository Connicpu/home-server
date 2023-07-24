use std::collections::BTreeMap;

use anyhow::bail;
use reqwest::StatusCode;
use serde::Deserialize;
use sycamore::{prelude::*, futures::spawn_local_scoped};
use web_sys::{window, Event};

use crate::{helpers::refresh_signal, auth::auth_token};

#[derive(Clone, Deserialize, PartialEq, Eq)]
struct UserStatus {
    level: i32,
    registered: bool,
}

#[component]
pub fn AdminPage(cx: Scope<'_>) -> View<DomNode> {
    let user_list = create_signal(cx, vec![]);
    spawn_local_scoped(cx, async move {
        refresh(user_list).await
    });

    let new_username = create_signal(cx, String::new());
    let create_user_err = create_signal(cx, String::new());
    let do_create_user = move |_e: Event| {
        create_user_err.set(String::new());

        let users = user_list.get();
        if users.is_empty() {
            create_user_err.set("Not Ready".into());
            return;
        }

        let username = new_username.get();
        if users.iter().any(|(user, _)| *user == *username) {
            create_user_err.set("Already Exists".into());
            return;
        }

        spawn_local_scoped(cx, async move {
            if username.len() < 1 {
                return;
            }

            if let Err(err) = create_user(&username).await {
                create_user_err.set(err.to_string());
                return;
            };

            refresh(user_list).await;
            new_username.set(String::new());
        });
    };

    view! { cx,
        h2(class = "page-title") { "Admin" }

        table {
            tr {
                th { "User" }
                th { "Registered" }
                th { "Auth Level" }
            }
            Keyed(
                iterable = user_list,
                key = move |&(ref k, _)| k.clone(),
                view = move |cx, (user, status)| {
                    let error_sig = create_signal(cx, String::new());

                    let level_sig = create_signal(cx, status.level.to_string());
                    {
                        let user = user.clone();
                        create_effect(cx, move || {
                            let new_level: i32 = level_sig.get().parse().unwrap();
                            if new_level == status.level {
                                return;
                            }
                            let user = user.clone();
                            spawn_local_scoped(cx, async move {
                                if let Err(err) = update_user_level(&user, new_level).await {
                                    error_sig.set(err.to_string());
                                    level_sig.set(status.level.to_string());
                                    return;
                                }
                            });
                        });
                    }

                    let registered_sig = create_signal(cx, status.registered);

                    let user_ = user.clone();
                    let do_reset_password = move |_e: Event| {
                        let user = user_.clone();
                        spawn_local_scoped(cx, async move {
                            if let Err(err) = reset_password(&user).await {
                                error_sig.set(err.to_string());
                            } else {
                                registered_sig.set(false);
                                refresh(user_list).await;
                            }
                        })
                    };
                    
                    let user_ = user.clone();
                    let do_delete_user = move |_e: Event| {
                        let user = user_.clone();
                        spawn_local_scoped(cx, async move {
                            if let Err(err) = delete_user(&user).await {
                                error_sig.set(err.to_string());
                            } else {
                                refresh(user_list).await;
                            }
                        })
                    };

                    view! { cx,
                        tr {
                            td { (user) }
                            td { (registered_sig.get()) }
                            td {
                                select(bind:value=level_sig) {
                                    option(value="0", selected=*level_sig.get() == "0") { "Read Only" }
                                    option(value="1", selected=*level_sig.get() == "1") { "Quick Access" }
                                    option(value="2", selected=*level_sig.get() == "2") { "Reprogram" }
                                    option(value="3", selected=*level_sig.get() == "3") { "Admin" }
                                }
                            }
                            td {
                                button(on:click=do_reset_password) {
                                    "Reset Password"
                                }
                                button(on:click=do_delete_user) {
                                    "Delete"
                                }
                                span(style="color:red") {
                                    (error_sig.get())
                                }
                            }
                        }
                    }
                }
            )
        }

        div {
            input(bind:value=new_username, placeholder="New Username...", style="width:200px")
            input(type="button", value="Create User", on:click=do_create_user)
            span(style="color:red") {
                (create_user_err.get())
            }
        }
    }
}

async fn refresh(user_list: &Signal<Vec<(String, UserStatus)>>) {
    web_sys::console::log_1(&format!("Refreshing user list").into());
    refresh_signal("auth/list_users", user_list, |x: BTreeMap<String, UserStatus>| x.into_iter().collect()).await
}

async fn update_user_level(user: &str, level: i32) -> anyhow::Result<()> {
    let window = window().unwrap();
    if user == "connie" && level < 3 {
        bail!("Not Allowed!");
    }
    
    let base = window.origin();
    let response = reqwest::Client::new()
        .put(format!("{base}/api/auth/auth_level"))
        .header("X-Auth", auth_token())
        .header("X-Username", user)
        .header("X-AuthLevel", level)
        .send()
        .await?;

    if response.status() != StatusCode::OK {
        bail!("Failed to update");
    }

    Ok(())
}

async fn reset_password(user: &str) -> anyhow::Result<()> {
    let window = window().unwrap();

    if !window.confirm_with_message(&format!("Are you sure you want to reset {user}'s password?")).unwrap() {
        bail!("");
    }
    
    let base = window.origin();
    let response = reqwest::Client::new()
        .put(format!("{base}/api/auth/reset_password"))
        .header("X-Auth", auth_token())
        .header("X-Username", user)
        .send()
        .await?;

    if response.status() != StatusCode::OK {
        bail!("Failed to reset");
    }

    Ok(())
}

async fn delete_user(user: &str) -> anyhow::Result<()> {
    let window = window().unwrap();
    if user == "connie" {
        bail!("Not Allowed >:(");
    }

    if !window.confirm_with_message(&format!("Are you sure you want to delete {user}")).unwrap() {
        bail!("");
    }

    let base = window.origin();
    let response = reqwest::Client::new()
        .delete(format!("{base}/api/auth/delete_user"))
        .header("X-Auth", auth_token())
        .header("X-Username", user)
        .send()
        .await?;
    
    if response.status() != StatusCode::OK {
        bail!("Failed to create user");
    }

    Ok(())
}

async fn create_user(user: &str) -> anyhow::Result<()> {
    let window = window().unwrap();
    let base = window.origin();
    let response = reqwest::Client::new()
        .put(format!("{base}/api/auth/auth_level"))
        .header("X-Auth", auth_token())
        .header("X-Username", user)
        .header("X-AuthLevel", 0)
        .send()
        .await?;
    
    if response.status() != StatusCode::OK {
        bail!("Failed to create user");
    }

    Ok(())
}
