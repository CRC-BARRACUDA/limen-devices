//! `devices` — a native Limen module that lists devices on **this**
//! machine across several buses.
//!
//! Provides `devices.local`. Methods: `list` (raw data), plus `ui`/`scan` for
//! the built-in view — the UI does **not** scan on open; it enumerates only when
//! the user presses **Scan**. Categories: **usb**, **pci**,
//! **monitor** (EDID), **disk** (non-USB), **net**, **bluetooth**. Enumeration is
//! per-OS; the platform code lives in [`linux`] / [`windows`], and every device is
//! emitted in one shared schema (see [`device`]):
//!
//! `category`, `type`, `id`, `vendor`, `product`, `serial`, `connected`.
//!
//! Built as a native (`cdylib`) module using `limen-sdk-rust`.

use limen_sdk_rust::ui::{button, label, separator, table, text, window};
use limen_sdk_rust::{export_module, json, rpc, Handler, Host, RpcError, Value};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux::list_devices;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows::list_devices;

/// Fallback for platforms without a collector.
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn list_devices() -> Value {
    json!({
        "os": std::env::consts::OS,
        "note": "device listing is only implemented for Windows and Linux",
        "devices": [],
    })
}

#[derive(Default)]
struct Devices;

impl Handler for Devices {
    fn capabilities(&self) -> Vec<String> {
        vec!["devices.local".into()]
    }

    fn invoke(
        &mut self,
        _capability: &str,
        method: &str,
        params: Value,
        _host: &Host,
    ) -> Result<Value, RpcError> {
        match method {
            // Initial view: a Scan button only — enumeration runs on demand.
            "ui" => Ok(idle_view()),
            // Scan now and render the results (also what Refresh calls).
            "scan" => Ok(ui_spec(&params)),
            "list" => Ok(list_devices()),
            other => Err(RpcError::new(
                rpc::METHOD_NOT_FOUND,
                format!("devices has no method {other}"),
            )),
        }
    }
}

/// Build one device record in the shared schema. Used by every platform collector.
pub(crate) fn device(
    category: &str,
    dtype: &str,
    id: String,
    vendor: Option<String>,
    product: Option<String>,
    serial: Option<String>,
    connected: bool,
) -> Value {
    json!({
        "category": category,
        "type": dtype,
        "id": id,
        "vendor": vendor,
        "product": product,
        "serial": serial,
        "connected": connected,
    })
}

/// The landing view: nothing is scanned until the user asks. Just a hint and a
/// Scan button that invokes `scan`.
fn idle_view() -> Value {
    window(
        "Local Devices",
        vec![
            label("Scan this machine for connected and previously-seen devices.").weak(),
            button("Scan", "devices.local", "scan").primary(),
        ],
    )
}

/// The results view: a search box + Refresh, then two sections (Connected /
/// Disconnected). Runs the scan. `params.query` filters; Refresh re-scans.
fn ui_spec(params: &Value) -> Value {
    let query = params
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase();

    let data = list_devices();
    let empty = Vec::new();
    let devices = data.get("devices").and_then(Value::as_array).unwrap_or(&empty);

    let cell = |d: &Value, key: &str| -> String {
        d.get(key).and_then(Value::as_str).unwrap_or("").to_string()
    };
    // Case-insensitive substring match across all fields.
    let matches = |d: &Value| -> bool {
        if query.is_empty() {
            return true;
        }
        let hay = format!(
            "{} {} {} {} {} {}",
            cell(d, "category"),
            cell(d, "type"),
            cell(d, "id"),
            cell(d, "vendor"),
            cell(d, "product"),
            cell(d, "serial"),
        )
        .to_lowercase();
        hay.contains(&query)
    };
    let row = |d: &Value| -> Vec<String> {
        vec![
            cell(d, "category"),
            cell(d, "type"),
            cell(d, "id"),
            cell(d, "vendor"),
            cell(d, "product"),
            cell(d, "serial"),
        ]
    };
    let rows_for = |connected: bool| -> Vec<Vec<String>> {
        devices
            .iter()
            .filter(|d| d.get("connected").and_then(Value::as_bool).unwrap_or(false) == connected)
            .filter(|d| matches(d))
            .map(row)
            .collect()
    };

    let cols: Vec<String> = ["Category", "Type", "ID", "Vendor", "Product", "Serial"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let connected = rows_for(true);
    let disconnected = rows_for(false);

    window(
        "Local Devices",
        vec![
            text("query")
                .label("Search")
                .placeholder("category, type, id, vendor, product, serial…")
                .default(query.clone()),
            button("Refresh", "devices.local", "scan").primary(),
            separator(),
            label(format!("Connected ({})", connected.len())).strong(),
            table(cols.clone(), connected),
            separator(),
            label(format!("Disconnected — previously connected ({})", disconnected.len())).strong(),
            table(cols, disconnected),
        ],
    )
}

export_module!(Devices);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_devices_across_categories() {
        let v = list_devices();
        let devs = v.get("devices").and_then(Value::as_array).expect("devices array");
        let mut counts = std::collections::BTreeMap::<String, usize>::new();
        for d in devs {
            assert!(d.get("category").and_then(Value::as_str).is_some());
            assert!(d.get("type").is_some());
            assert!(d.get("connected").is_some());
            let c = d.get("category").and_then(Value::as_str).unwrap_or("?");
            *counts.entry(c.to_string()).or_default() += 1;
        }
        eprintln!("total = {}, by category = {counts:?}", devs.len());
    }
}
