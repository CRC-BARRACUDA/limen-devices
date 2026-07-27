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

use std::collections::HashMap;

use limen_sdk_rust::ui::{
    button, label, menu_item, row, select, separator, table, text, window, MenuItem,
};
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
struct Devices {
    /// Whether the user has scanned this session. Once true, reopening the tab
    /// shows the saved results instead of the landing Scan button.
    scanned: bool,
    /// The raw search text from the last scan (restored into the search box).
    last_query: String,
    /// The full device list from the last scan, so the view can be re-rendered
    /// on reopen without re-enumerating the machine.
    last_devices: Vec<Value>,
    /// The last scan, keyed by the row id sent back on a row action, so
    /// `about` / `open_path` can resolve which device the user acted on.
    last: HashMap<String, Value>,
}

impl Handler for Devices {
    fn capabilities(&self) -> Vec<String> {
        vec!["devices.local".into()]
    }

    fn invoke(
        &mut self,
        _capability: &str,
        method: &str,
        params: Value,
        host: &Host,
    ) -> Result<Value, RpcError> {
        // Optional integration: only offer "Make Report" when a report provider
        // is actually loaded (discovered at call time, never a hard dependency).
        let report = host.has_capability("report.build");
        match method {
            // Landing view: the saved results if the user has scanned this
            // session, otherwise just a Scan button (no enumeration on open).
            "ui" => Ok(if self.scanned {
                let devices = self.last_devices.clone();
                let query = self.last_query.clone();
                self.render(&devices, &query, report)
            } else {
                idle_view()
            }),
            // Scan now (enumerate), save the state, and render (also Refresh).
            "scan" => Ok(self.scan(&params, report)),
            "list" => Ok(list_devices()),
            // Row actions: open a device's details, or open its OS location.
            "about" => Ok(self.about(&params)),
            "open_path" => Ok(self.open_path(&params, host)),
            // Report integration (present only while a report provider is loaded).
            "report_config" => Ok(report_config()),
            "make_report" => Ok(self.make_report(&params, host)),
            other => Err(RpcError::new(
                rpc::METHOD_NOT_FOUND,
                format!("devices has no method {other}"),
            )),
        }
    }
}

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

/// A cell value (empty string if the field is missing).
fn cell(d: &Value, key: &str) -> String {
    d.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// The six visible columns for a device row.
fn row_cells(d: &Value) -> Vec<String> {
    ["category", "type", "id", "vendor", "product", "serial"]
        .iter()
        .map(|k| cell(d, k))
        .collect()
}

/// The right-click menu shared by every device row. The activated row's id is
/// added by the host, so each entry only needs its `target`. On Windows a device
/// lives in the Registry or Device Manager — never on the filesystem — so those
/// are the only two destinations; Linux has a single path entry.
fn row_menu() -> Vec<MenuItem> {
    let mut items = vec![menu_item("About device", "devices.local", "about").open_in_tab()];
    #[cfg(target_os = "windows")]
    {
        items.push(limen_sdk_rust::ui::submenu(
            "Open in",
            vec![
                menu_item("Registry", "devices.local", "open_path")
                    .args(json!({ "target": "registry" })),
                menu_item("Device Manager", "devices.local", "open_path")
                    .args(json!({ "target": "device_manager" })),
            ],
        ));
    }
    #[cfg(not(target_os = "windows"))]
    {
        items.push(
            menu_item("Open path", "devices.local", "open_path").args(json!({ "target": "path" })),
        );
    }
    items
}

/// The "Make Report" configuration view (opened in a tab): choose the output,
/// what to include, and which devices, then Generate.
fn report_config() -> Value {
    let opts = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    window(
        "Make Report",
        vec![
            label("Report options").strong(),
            select("format", opts(&["In-app view", "Markdown", "HTML", "CSV"])).label("Output"),
            select("content", opts(&["Tables and charts", "Tables only", "Charts only"]))
                .label("Include"),
            select("scope", opts(&["All devices", "Connected only", "Disconnected only"]))
                .label("Devices"),
            button("Generate", "devices.local", "make_report").primary().open_in_tab(),
        ],
    )
}

impl Devices {
    /// Enumerate the machine, save the scan state (so reopening the tab restores
    /// it), and render. `params.query` filters; Refresh calls this again.
    fn scan(&mut self, params: &Value, report: bool) -> Value {
        let query = params.get("query").and_then(Value::as_str).unwrap_or("").to_string();
        let data = list_devices();
        let devices: Vec<Value> = data
            .get("devices")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        self.scanned = true;
        self.last_devices = devices.clone();
        self.last_query = query.clone();
        self.render(&devices, &query, report)
    }

    /// The results view: a search box + Refresh (+ Make Report when a report
    /// provider is loaded), then two sections (Connected / Disconnected). Filters
    /// `devices` by `query_raw` and caches each shown device by its row id so row
    /// actions (`about` / `open_path`) resolve it.
    fn render(&mut self, devices: &[Value], query_raw: &str, report: bool) -> Value {
        let query = query_raw.to_lowercase();
        let matches = |d: &Value| -> bool {
            if query.is_empty() {
                return true;
            }
            row_cells(d).join(" ").to_lowercase().contains(&query)
        };

        self.last.clear();
        let (mut conn_rows, mut conn_ids) = (Vec::new(), Vec::new());
        let (mut disc_rows, mut disc_ids) = (Vec::new(), Vec::new());
        for (i, d) in devices.iter().enumerate() {
            if !matches(d) {
                continue;
            }
            let rid = i.to_string();
            self.last.insert(rid.clone(), d.clone());
            if d.get("connected").and_then(Value::as_bool).unwrap_or(false) {
                conn_ids.push(rid);
                conn_rows.push(row_cells(d));
            } else {
                disc_ids.push(rid);
                disc_rows.push(row_cells(d));
            }
        }

        let cols: Vec<String> = ["Category", "Type", "ID", "Vendor", "Product", "Serial"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let menu = row_menu();

        // Toolbar: Refresh, and Make Report only when a report provider is loaded.
        let mut actions = vec![button("Refresh", "devices.local", "scan").primary()];
        if report {
            actions.push(button("Make Report", "devices.local", "report_config").open_in_tab());
        }

        window(
            "Local Devices",
            vec![
                text("query")
                    .label("Search")
                    .placeholder("category, type, id, vendor, product, serial…")
                    .default(query_raw.to_string()),
                row(actions),
                label("Right-click a row for actions; double-click to open its details.").weak(),
                separator(),
                label(format!("Connected ({})", conn_rows.len())).strong(),
                table(cols.clone(), conn_rows)
                    .row_ids(conn_ids)
                    .row_menu(menu.clone())
                    .on_activate("devices.local", "about"),
                separator(),
                label(format!("Disconnected — previously connected ({})", disc_rows.len())).strong(),
                table(cols, disc_rows)
                    .row_ids(disc_ids)
                    .row_menu(menu)
                    .on_activate("devices.local", "about"),
            ],
        )
    }

    /// Build a report spec from the last scan and hand it to a report provider.
    /// `params` come from the config view's selects (`format`/`content`/`scope`).
    fn make_report(&self, params: &Value, host: &Host) -> Value {
        let fmt = match params.get("format").and_then(Value::as_str).unwrap_or("") {
            "Markdown" => "markdown",
            "HTML" => "html",
            "CSV" => "csv",
            _ => "view",
        };
        let content = params.get("content").and_then(Value::as_str).unwrap_or("");
        let scope = params.get("scope").and_then(Value::as_str).unwrap_or("");
        let spec = self.report_spec(fmt, content, scope);
        match host.call("report.build", "build", spec) {
            // The report provider returned a view — show it in this tab.
            Ok(v) if v.get("widgets").is_some() => v,
            // An export (file written + opened) acknowledges with null.
            Ok(_) => window(
                "Report",
                vec![
                    label("Report exported").strong(),
                    label("The document was generated and opened in your default app.").weak(),
                ],
            ),
            Err(e) => window(
                "Report",
                vec![
                    label("Couldn't build the report").strong(),
                    label(format!("{e}")).weak(),
                ],
            ),
        }
    }

    /// Assemble the report spec (title, summary, a category chart, and Connected
    /// / Disconnected tables) from the last scan, honoring the config choices.
    fn report_spec(&self, fmt: &str, content: &str, scope: &str) -> Value {
        let devices = &self.last_devices;
        let is_conn = |d: &Value| d.get("connected").and_then(Value::as_bool).unwrap_or(false);
        let in_scope = |d: &Value| match scope {
            "Connected only" => is_conn(d),
            "Disconnected only" => !is_conn(d),
            _ => true,
        };
        let total = devices.len();
        let connected = devices.iter().filter(|d| is_conn(d)).count();

        let mut counts: HashMap<String, i64> = HashMap::new();
        for d in devices.iter().filter(|d| in_scope(d)) {
            *counts.entry(cell(d, "category")).or_default() += 1;
        }
        let mut chart_pairs: Vec<(String, i64)> = counts.into_iter().collect();
        chart_pairs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let chart_data: Vec<Value> = chart_pairs
            .iter()
            .map(|(k, v)| json!({ "label": k, "value": v }))
            .collect();

        let cols = ["Category", "Type", "ID", "Vendor", "Product", "Serial"];
        let section = |heading: &str, want: Option<bool>| -> Value {
            let rows: Vec<Vec<String>> = devices
                .iter()
                .filter(|d| in_scope(d))
                .filter(|d| want.is_none_or(|w| is_conn(d) == w))
                .map(row_cells)
                .collect();
            json!({ "heading": heading, "columns": cols, "rows": rows })
        };

        let mut charts = Vec::new();
        if content != "Tables only" && !chart_data.is_empty() {
            charts.push(json!({ "title": "Devices by category", "data": chart_data }));
        }
        let mut sections = Vec::new();
        if content != "Charts only" {
            match scope {
                "Connected only" => sections.push(section("Connected", Some(true))),
                "Disconnected only" => sections.push(section("Disconnected", Some(false))),
                _ => {
                    sections.push(section("Connected", Some(true)));
                    sections.push(section("Disconnected — previously connected", Some(false)));
                }
            }
        }

        json!({
            "title": "Device Report",
            "subtitle": format!("{total} devices · {connected} connected"),
            "format": fmt,
            "summary": [
                format!("Total devices: {total}"),
                format!("Connected now: {connected}"),
            ],
            "charts": charts,
            "sections": sections,
        })
    }

    /// A detail view for one device (opened in a new tab from a row action).
    fn about(&self, params: &Value) -> Value {
        let id = params.get("id").and_then(Value::as_str).unwrap_or("");
        let Some(d) = self.last.get(id) else {
            return window(
                "Device",
                vec![label("This device isn't in the latest scan — re-scan and try again.").weak()],
            );
        };
        let shown = |v: String| if v.is_empty() { "—".to_string() } else { v };
        let field = |name: &str, val: String| {
            row(vec![label(name.to_string()).strong(), label(shown(val))])
        };
        let title = {
            let p = cell(d, "product");
            if !p.is_empty() {
                p
            } else {
                let v = cell(d, "vendor");
                if v.is_empty() { cell(d, "id") } else { v }
            }
        };
        let connected = d.get("connected").and_then(Value::as_bool).unwrap_or(false);

        let mut widgets = vec![
            label(title.clone()).strong(),
            separator(),
            field("Category", cell(d, "category")),
            field("Type", cell(d, "type")),
            field("ID", cell(d, "id")),
            field("Vendor", cell(d, "vendor")),
            field("Product", cell(d, "product")),
            field("Serial", cell(d, "serial")),
            field("Connected", if connected { "yes".into() } else { "no".into() }),
        ];
        let path = cell(d, "path");
        if !path.is_empty() {
            widgets.push(field("Location", path.clone()));
        }
        widgets.push(separator());

        // Open actions carry the id + target so they don't rely on the row menu.
        #[cfg(target_os = "windows")]
        {
            widgets.push(
                button("Open in Registry", "devices.local", "open_path")
                    .args(json!({ "id": id, "target": "registry" })),
            );
            widgets.push(
                button("Open in Device Manager", "devices.local", "open_path")
                    .args(json!({ "id": id, "target": "device_manager" })),
            );
        }
        #[cfg(not(target_os = "windows"))]
        {
            if !path.is_empty() {
                widgets.push(
                    button("Open path", "devices.local", "open_path")
                        .args(json!({ "id": id, "target": "path" }))
                        .primary(),
                );
            }
        }

        window(title, widgets)
    }

    /// Open a device's OS location: file manager (Linux), or Registry / Device
    /// Manager (Windows). `params`: `{ id, target }`.
    fn open_path(&self, params: &Value, host: &Host) -> Value {
        let id = params.get("id").and_then(Value::as_str).unwrap_or("");
        let target = params.get("target").and_then(Value::as_str).unwrap_or("path");
        // Each destination needs a different value: regedit navigates to the
        // device's registry key, Device Manager wants its instance id. Sending
        // `path` to both — as this did — meant Device Manager got a registry
        // key it could not resolve.
        let field = match target {
            "device_manager" => "instance_id",
            _ => "path",
        };
        let value = self
            .last
            .get(id)
            .map(|d| cell(d, field))
            .unwrap_or_default();
        host.open(target, &value);
        // Fire-and-forget: no Result pane for this action.
        Value::Null
    }
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
