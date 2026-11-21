use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn probe() -> Result<String, JsValue> {
    Ok("{}".into())
}
