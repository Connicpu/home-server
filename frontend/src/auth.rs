use std::{sync::Arc, time::Duration};

use arc_cell::OptionalArcCell;
use chrono::{DateTime, Utc};
use gloo_timers::future::sleep;
use jwt::{Token, Header};
use reqwest::StatusCode;
use serde::{Serialize, Deserialize};
use sycamore::{prelude::*, futures::ScopeSpawnLocal};
use web_sys::{window, Event};

static AUTH_TOKEN: OptionalArcCell<String> = OptionalArcCell::const_new();
pub fn auth_token() -> String {
    AUTH_TOKEN.get().map(|s| (*s).clone()).unwrap_or_default()
}

#[component]
pub fn LoginForm<'a, G: Html>(cx: ScopeRef<'a>, logged_in: &'a Signal<Option<bool>>) -> View<G> {
    let username = cx.create_signal(String::new());
    let password = cx.create_signal(String::new());
    let problem = cx.create_signal(String::new());

    let do_login = on_login(cx, username, password, problem, logged_in);

    view! { cx,
        div(class="login-form") {
            div {
                input(
                    bind:value=username,
                    type="email",
                    placeholder="Username..."
                )
            }
            div {
                input(
                    bind:value=password,
                    type="password",
                    placeholder="Password..."
                )
            }
            div {
                input(
                    value="Login",
                    type="button",
                    on:click=do_login
                )
            }
            div(class="login-problem") {
                (problem.get())
            }
        }
    }
}

fn on_login<'a>(
    cx: ScopeRef<'a>,
    username: &'a Signal<String>,
    password: &'a Signal<String>,
    problem: &'a Signal<String>,
    logged_in: &'a Signal<Option<bool>>,
) -> impl Fn(Event) + 'a {
    move |_: Event| {
        problem.set("".into());

        let username = username.get();
        let password = password.get();

        cx.spawn_local(async move {
            match login(&username, &password).await {
                Ok(auth_token) => {
                    if let Some(local_storage) = window().unwrap().local_storage().unwrap() {
                        local_storage.set_item("auth-token", &auth_token).unwrap();
                    }

                    AUTH_TOKEN.set(Some(Arc::new(auth_token)));
                    logged_in.set(Some(true));
                }
                Err(err) => {
                    problem.set(format!("{:?}", err));
                }
            }
        });
    }
}

pub async fn check_logged_in(logged_in: &Signal<Option<bool>>) {
    let Some(local_storage) = window().unwrap().local_storage().unwrap() else {
        logged_in.set(Some(false));
        return;
    };

    let Some(auth_token) = local_storage.get_item("auth-token").unwrap() else {
        logged_in.set(Some(false));
        return;
    };

    if auth_token.is_empty() {
        logged_in.set(Some(false));
        return;
    }

    AUTH_TOKEN.set(Some(Arc::new(auth_token.clone())));
    logged_in.set(Some(true));

    let base = window().unwrap().origin();
    let Ok(response) = reqwest::Client::new()
        .get(format!("{base}/api/auth/renew"))
        .header("X-Auth", &auth_token)
        .send()
        .await else {
            logged_in.set(Some(false));
            return;
        };

    if response.status() != StatusCode::OK {
        AUTH_TOKEN.set(None);
        logged_in.set(Some(false));
        return;
    }

    let Ok(new_token) = response.text().await else {
        AUTH_TOKEN.set(None);
        logged_in.set(Some(false));
        return;
    };

    local_storage.set_item("auth-token", &new_token).unwrap();
    AUTH_TOKEN.set(Some(Arc::new(new_token)));
    logged_in.set(Some(true));
}

pub async fn logout(logged_in: &Signal<Option<bool>>) {
    logged_in.set(None);
    sleep(Duration::from_secs(1)).await;

    if let Some(local_storage) = window().unwrap().local_storage().unwrap() {
        local_storage.remove_item("auth-token").unwrap();
    }

    let Some(auth_token) = AUTH_TOKEN.get() else {
        return
    };

    let base = window().unwrap().origin();
    reqwest::Client::new()
        .get(format!("{base}/api/auth/logout"))
        .header("X-Auth", &*auth_token)
        .send()
        .await
        .ok();

    logged_in.set(Some(false));
}

async fn login(username: &str, password: &str) -> anyhow::Result<String> {
    let base = window().unwrap().origin();
    let response = reqwest::Client::new()
        .get(format!("{base}/api/auth/login"))
        .header("X-Username", username)
        .header("X-Password", password)
        .send()
        .await?;

    if response.status() != StatusCode::OK {
        anyhow::bail!("Incorrect username/password");
    }

    Ok(response.text().await?)
}

pub fn is_auth_level(level: i32) -> bool {
    let Some(token) = AUTH_TOKEN.get() else { return false; };
    let Ok(token): Result<Token<Header, Authentication, _>, _> = Token::parse_unverified(&token) else { return false; };

    token.claims().auth_level >= level
}

#[derive(Clone, Serialize, Deserialize)]
struct Authentication {
    user: String,
    valid_until: DateTime<Utc>,
    auth_level: i32,
}
