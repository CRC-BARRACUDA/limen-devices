//! One device, in the shared schema every collector emits — and the two ways a
//! view reads it back.

use crate::*;

/// Build one device record in the shared schema. Used by every platform
/// collector. `path` is an OS location for the device (a sysfs directory on
/// Linux), used by the "Open path" row action; `None` when there isn't one.
#[allow(clippy::too_many_arguments)]
pub(crate) fn device(
    category: &str,
    dtype: &str,
    id: String,
    vendor: Option<String>,
    product: Option<String>,
    serial: Option<String>,
    connected: bool,
    path: Option<String>,
) -> Value {
    json!({
        "category": category,
        "type": dtype,
        "id": id,
        "vendor": vendor,
        "product": product,
        "serial": serial,
        "connected": connected,
        "path": path,
    })
}

/// A cell value (empty string if the field is missing).
pub(crate) fn cell(d: &Value, key: &str) -> String {
    d.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// The six visible columns for a device row.
pub(crate) fn row_cells(d: &Value) -> Vec<String> {
    ["category", "type", "id", "vendor", "product", "serial"]
        .iter()
        .map(|k| cell(d, k))
        .collect()
}
