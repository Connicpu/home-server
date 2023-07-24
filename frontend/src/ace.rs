use wasm_bindgen::prelude::*;
use web_sys::HtmlElement;

#[wasm_bindgen]
extern {
    #[derive(Clone)]
    pub type Editor;
    #[wasm_bindgen(js_namespace = ace)]
    pub fn edit(element: HtmlElement, options: JsValue) -> Editor;
    #[wasm_bindgen(method, js_name = setTheme)]
    pub fn set_theme(this: &Editor, theme: &str);
    #[wasm_bindgen(method, js_name = getValue)]
    pub fn get_value(this: &Editor) -> String;
    #[wasm_bindgen(method, js_name = setValue)]
    pub fn set_value(this: &Editor, value: &str);
    #[wasm_bindgen(method, getter)]
    pub fn selection(this: &Editor) -> Selection;
    
    #[derive(Clone)]
    pub type Selection;
    #[wasm_bindgen(method, js_name = clearSelection)]
    pub fn clear_selection(this: &Selection);
}
