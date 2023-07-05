use sycamore::prelude::*;

#[component]
pub fn AdminPage(cx: Scope<'_>) -> View<DomNode> {
    view! { cx,
        h2(class = "page-title") { "Admin" }
    }
}
