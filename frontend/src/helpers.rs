use serde::{de::DeserializeOwned, Serialize};
use sycamore::prelude::*;
use web_sys::window;

pub fn create_saved_signal<'a, T>(cx: ScopeRef<'a>, name: &'static str, default: T) -> &'a Signal<T>
where
    T: Serialize + DeserializeOwned,
{
    fn get_value<T: DeserializeOwned>(name: &str) -> Option<T> {
        let ls = window().unwrap().local_storage().unwrap()?;
        let json = ls.get_item(&format!("saved-signal_{name}")).unwrap()?;
        serde_json::from_str(&json).ok()
    }

    fn set_value<T: Serialize>(name: &str, value: &T) {
        let Some(ls) = window().unwrap().local_storage().unwrap() else { return };
        let json = serde_json::to_string(&value).unwrap();
        ls.set_item(&format!("saved-signal_{name}"), &json).unwrap();
    }

    let initial = get_value(name).unwrap_or(default);

    let signal = cx.create_signal(initial);
    cx.create_effect(move || {
        set_value(name, &*signal.get());
    });

    signal
}
