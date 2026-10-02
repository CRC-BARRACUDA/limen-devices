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
use limen_sdk_rust::{export_module, json, rpc, Catalog, Handler, Host, RpcError, Value};

/// Every word this module shows, in each language it has.
///
/// English lives in a file beside the Ukrainian rather than in the code: a
/// string written into the source is a string nobody can translate.
fn catalog() -> &'static Catalog {
    static C: std::sync::OnceLock<Catalog> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        Catalog::new(&[
            ("en", include_str!("locales/en.toml")),
            ("uk", include_str!("locales/uk.toml")),
        ])
    })
}

/// Whether a dropdown's answer is this choice, in whichever language it was
/// shown in.
///
/// An option is its own value — a person reading Ukrainian sends Ukrainian
/// back — so the answer is compared against every language rather than against
/// the English it used to be.
fn chose(answer: &str, key: &str) -> bool {
    ["en", "uk"].iter().any(|lang| catalog().tr(lang, key) == answer)
}

/// The six visible columns, as catalogue keys — named once, so the table and
/// the report cannot drift apart.
const COLUMNS: [&str; 6] = [
    "col.category",
    "col.type",
    "col.id",
    "col.vendor",
    "col.product",
    "col.serial",
];

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
        let lang = host.locale();
        let lang = lang.as_str();
        match method {
            // Landing view: the saved results if the user has scanned this
            // session, otherwise just a Scan button (no enumeration on open).
            "ui" => Ok(if self.scanned {
                let devices = self.last_devices.clone();
                let query = self.last_query.clone();
                self.render(&devices, &query, report, lang)
            } else {
                idle_view(lang)
            }),
            // Scan now (enumerate), save the state, and render (also Refresh).
            "scan" => Ok(self.scan(&params, report, lang)),
            "list" => Ok(list_devices()),
            // Row actions: open a device's details, or open its OS location.
            "about" => Ok(self.about(&params, lang)),
            "open_path" => Ok(self.open_path(&params, host)),
            // Report integration (present only while a report provider is loaded).
            "report_config" => Ok(report_config(lang)),
            "make_report" => Ok(self.make_report(&params, host, lang)),
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
fn idle_view(lang: &str) -> Value {
    let t = |k: &str| catalog().tr(lang, k);
    window(
        t("ui.title"),
        vec![
            label(t("ui.idle_hint")).weak(),
            button(t("ui.scan"), "devices.local", "scan").primary(),
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
fn row_menu(lang: &str) -> Vec<MenuItem> {
    let t = |k: &str| catalog().tr(lang, k);
    let mut items = vec![menu_item(t("menu.about"), "devices.local", "about").open_in_tab()];
    #[cfg(target_os = "windows")]
    {
        items.push(limen_sdk_rust::ui::submenu(
            t("menu.open_in"),
            vec![
                menu_item(t("menu.registry"), "devices.local", "open_path")
                    .args(json!({ "target": "registry" })),
                menu_item(t("menu.device_manager"), "devices.local", "open_path")
                    .args(json!({ "target": "device_manager" })),
            ],
        ));
    }
    #[cfg(not(target_os = "windows"))]
    {
        items.push(
            menu_item(t("menu.open_path"), "devices.local", "open_path")
                .args(json!({ "target": "path" })),
        );
    }
    items
}

/// The "Make Report" configuration view (opened in a tab): choose the output,
/// what to include, and which devices, then Generate.
fn report_config(lang: &str) -> Value {
    let t = |k: &str| catalog().tr(lang, k);
    let opts = |keys: &[&str]| keys.iter().map(|k| t(k)).collect::<Vec<_>>();
    window(
        t("report.config_title"),
        vec![
            label(t("report.options")).strong(),
            select(
                "content",
                opts(&[
                    "report.content_both",
                    "report.content_tables",
                    "report.content_charts",
                ]),
            )
            .label(t("report.include")),
            select(
                "scope",
                opts(&[
                    "report.scope_all",
                    "report.scope_connected",
                    "report.scope_disconnected",
                ]),
            )
            .label(t("report.devices")),
            // Not `open_in_tab`: the report provider answers with a pop-up,
            // and one opened into a tab of its own leaves that tab empty.
            button(t("report.generate"), "devices.local", "make_report").primary(),
        ],
    )
}

impl Devices {
    /// Enumerate the machine, save the scan state (so reopening the tab restores
    /// it), and render. `params.query` filters; Refresh calls this again.
    fn scan(&mut self, params: &Value, report: bool, lang: &str) -> Value {
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
        self.render(&devices, &query, report, lang)
    }

    /// The results view: a search box + Refresh (+ Make Report when a report
    /// provider is loaded), then two sections (Connected / Disconnected). Filters
    /// `devices` by `query_raw` and caches each shown device by its row id so row
    /// actions (`about` / `open_path`) resolve it.
    fn render(
        &mut self,
        devices: &[Value],
        query_raw: &str,
        report: bool,
        lang: &str,
    ) -> Value {
        let t = |k: &str| catalog().tr(lang, k);
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

        let cols: Vec<String> = COLUMNS.iter().map(|k| t(k)).collect();
        let menu = row_menu(lang);

        // Toolbar: Refresh, and Make Report only when a report provider is loaded.
        let mut actions = vec![button(t("ui.refresh"), "devices.local", "scan").primary()];
        if report {
            actions.push(
                button(t("ui.report"), "devices.local", "report_config").open_in_tab(),
            );
        }

        window(
            t("ui.title"),
            vec![
                text("query")
                    .label(t("ui.search"))
                    .placeholder(t("ui.search_ph"))
                    .default(query_raw.to_string()),
                row(actions),
                label(t("ui.rows_hint")).weak(),
                separator(),
                label(t("ui.connected").replace("{n}", &conn_rows.len().to_string())).strong(),
                table(cols.clone(), conn_rows)
                    .row_ids(conn_ids)
                    .row_menu(menu.clone())
                    .on_activate("devices.local", "about"),
                separator(),
                label(t("ui.disconnected").replace("{n}", &disc_rows.len().to_string())).strong(),
                table(cols, disc_rows)
                    .row_ids(disc_ids)
                    .row_menu(menu)
                    .on_activate("devices.local", "about"),
            ],
        )
    }

    /// Build a report spec from the last scan and hand it to a report provider.
    /// `params` come from the config view's selects (`format`/`content`/`scope`).
    fn make_report(&self, params: &Value, host: &Host, lang: &str) -> Value {
        let t = |k: &str| catalog().tr(lang, k);
        // Always the preview: which file it becomes — PDF, Word, a page, a
        // table — is chosen there, beside the thing being saved, rather than in
        // a dropdown here that has to be kept in step with what the report
        // module can actually produce.
        let fmt = "view";
        let content = params.get("content").and_then(Value::as_str).unwrap_or("");
        let scope = params.get("scope").and_then(Value::as_str).unwrap_or("");
        let spec = self.report_spec(fmt, content, scope, lang);
        match host.call("report.build", "build", spec) {
            // The report provider returned a view — show it in this tab.
            Ok(v) if v.get("widgets").is_some() => v,
            // An export (file written + opened) acknowledges with null.
            Ok(_) => window(
                t("report.config_title"),
                vec![
                    label(t("report.exported")).strong(),
                    label(t("report.exported_body")).weak(),
                ],
            ),
            Err(e) => window(
                t("report.config_title"),
                vec![label(t("report.failed")).strong(), label(format!("{e}")).weak()],
            ),
        }
    }

    /// Assemble the report spec (title, summary, a category chart, and Connected
    /// / Disconnected tables) from the last scan, honoring the config choices.
    fn report_spec(&self, fmt: &str, content: &str, scope: &str, lang: &str) -> Value {
        let t = |k: &str| catalog().tr(lang, k);
        let devices = &self.last_devices;
        let is_conn = |d: &Value| d.get("connected").and_then(Value::as_bool).unwrap_or(false);
        let in_scope = |d: &Value| {
            if chose(scope, "report.scope_connected") {
                is_conn(d)
            } else if chose(scope, "report.scope_disconnected") {
                !is_conn(d)
            } else {
                true
            }
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

        let cols: Vec<String> = COLUMNS.iter().map(|k| t(k)).collect();
        let section = |heading: &str, want: Option<bool>| -> Value {
            let rows: Vec<Vec<String>> = devices
                .iter()
                .filter(|d| in_scope(d))
                .filter(|d| want.is_none_or(|w| is_conn(d) == w))
                .map(row_cells)
                .collect();
            json!({ "heading": heading, "columns": cols.clone(), "rows": rows })
        };

        let mut charts = Vec::new();
        if !chose(content, "report.content_tables") && !chart_data.is_empty() {
            charts.push(json!({ "title": t("report.chart"), "data": chart_data }));
        }
        let mut sections = Vec::new();
        if !chose(content, "report.content_charts") {
            let (conn, disc) = (t("report.section_connected"), t("report.section_disconnected"));
            if chose(scope, "report.scope_connected") {
                sections.push(section(&conn, Some(true)));
            } else if chose(scope, "report.scope_disconnected") {
                sections.push(section(&disc, Some(false)));
            } else {
                sections.push(section(&conn, Some(true)));
                sections.push(section(&disc, Some(false)));
            }
        }

        json!({
            "title": t("report.title"),
            "subtitle": t("report.subtitle")
                .replace("{total}", &total.to_string())
                .replace("{connected}", &connected.to_string()),
            "format": fmt,
            "summary": [
                t("report.total").replace("{n}", &total.to_string()),
                t("report.connected_now").replace("{n}", &connected.to_string()),
            ],
            "charts": charts,
            "sections": sections,
        })
    }

    /// A detail view for one device (opened in a new tab from a row action).
    fn about(&self, params: &Value, lang: &str) -> Value {
        let t = |k: &str| catalog().tr(lang, k);
        let id = params.get("id").and_then(Value::as_str).unwrap_or("");
        let Some(d) = self.last.get(id) else {
            return window(t("detail.title"), vec![label(t("detail.missing")).weak()]);
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
            field(&t("col.category"), cell(d, "category")),
            field(&t("col.type"), cell(d, "type")),
            field(&t("col.id"), cell(d, "id")),
            field(&t("col.vendor"), cell(d, "vendor")),
            field(&t("col.product"), cell(d, "product")),
            field(&t("col.serial"), cell(d, "serial")),
            field(
                &t("detail.connected"),
                t(if connected { "val.yes" } else { "val.no" }),
            ),
        ];
        let path = cell(d, "path");
        if !path.is_empty() {
            widgets.push(field(&t("detail.location"), path.clone()));
        }
        widgets.push(separator());

        // Open actions carry the id + target so they don't rely on the row menu.
        #[cfg(target_os = "windows")]
        {
            widgets.push(
                button(t("detail.open_registry"), "devices.local", "open_path")
                    .args(json!({ "id": id, "target": "registry" })),
            );
            widgets.push(
                button(t("detail.open_device_manager"), "devices.local", "open_path")
                    .args(json!({ "id": id, "target": "device_manager" })),
            );
        }
        #[cfg(not(target_os = "windows"))]
        {
            if !path.is_empty() {
                widgets.push(
                    button(t("detail.open_path"), "devices.local", "open_path")
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

/// The catalogue, and that both languages actually say everything.
#[cfg(test)]
mod i18n_tests {
    use super::*;

    /// One device, as a scan would record it.
    fn seen(id: &str, connected: bool) -> Value {
        json!({
            "category": "usb", "type": "storage", "id": id,
            "vendor": "Acme", "product": "Stick", "serial": "SN1",
            "connected": connected, "path": "",
        })
    }

    fn scanned(devices: Vec<Value>) -> Devices {
        Devices { last_devices: devices, scanned: true, ..Default::default() }
    }

    /// Every `a.b` key a locale file defines, read from the file rather than
    /// through the catalogue: `tr` falls back to English for a key Ukrainian is
    /// missing, so asking it would hide exactly what this is looking for.
    fn keys(src: &str) -> Vec<String> {
        let mut table = String::new();
        let mut out = Vec::new();
        for line in src.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                table = name.to_string();
            } else if let Some((key, _)) = line.split_once(" = ") {
                out.push(format!("{table}.{key}"));
            }
        }
        out
    }

    /// Ukrainian says everything English says. A key only one of them has is a
    /// screen that falls back to English mid-sentence.
    #[test]
    fn both_languages_say_the_same_things() {
        let en = keys(include_str!("locales/en.toml"));
        let uk = keys(include_str!("locales/uk.toml"));
        assert!(en.len() > 40, "the catalogue is suspiciously small");
        for key in &en {
            assert!(uk.contains(key), "uk.toml is missing {key}");
        }
        for key in uk.iter().filter(|k| !k.starts_with("module.")) {
            assert!(en.contains(key), "en.toml is missing {key}");
        }
    }

    /// A screen asked for in Ukrainian comes back in Ukrainian — not a mix, and
    /// not English with a translated title.
    #[test]
    fn the_screens_are_translated_not_merely_titled() {
        let idle = idle_view("uk").to_string();
        assert!(idle.contains("Сканувати"), "{idle}");
        assert!(!idle.contains("Scan this machine"), "English survived: {idle}");

        let cfg = report_config("uk").to_string();
        for word in ["Параметри звіту", "Лише таблиці", "Лише під'єднані", "Створити"] {
            assert!(cfg.contains(word), "{word} is missing from {cfg}");
        }
        assert!(!cfg.contains("Tables only"), "English survived: {cfg}");

        let mut m = scanned(vec![seen("a", true)]);
        m.last.insert("0".into(), seen("a", true));
        let about = m.about(&json!({ "id": "0" }), "uk").to_string();
        for word in ["Категорія", "Виробник", "Під'єднано"] {
            assert!(about.contains(word), "{word} is missing from {about}");
        }

        let table = m.render(&[seen("a", true)], "", false, "uk").to_string();
        assert!(table.contains("Серійний номер"), "the columns: {table}");
        assert!(!table.contains("\"Serial\""), "English survived: {table}");
    }

    /// A dropdown's answer comes back in the language it was shown in, so the
    /// module has to recognise its own words — in either language, because a
    /// spec built elsewhere may still say "Connected only".
    #[test]
    fn a_choice_is_understood_in_the_language_it_was_made_in() {
        assert!(chose("Лише під'єднані", "report.scope_connected"));
        assert!(chose("Connected only", "report.scope_connected"));
        assert!(!chose("Лише від'єднані", "report.scope_connected"));

        let m = scanned(vec![seen("on", true), seen("off", false)]);
        for answer in ["Лише під'єднані", "Connected only"] {
            let spec = m.report_spec("view", "", answer, "uk");
            let sections = spec["sections"].as_array().unwrap();
            assert_eq!(sections.len(), 1, "{answer} did not narrow the report");
            assert_eq!(sections[0]["rows"].as_array().unwrap().len(), 1);
        }
    }

    /// The report a Ukrainian screen asks for is a Ukrainian report: its title,
    /// its headings and its columns, not only the rows it carries.
    #[test]
    fn the_report_speaks_the_language_it_was_asked_in() {
        let spec = scanned(vec![seen("a", true), seen("b", false)])
            .report_spec("view", "", "", "uk");
        assert_eq!(spec["title"], "Звіт про пристрої");
        assert_eq!(spec["sections"][0]["heading"], "Під'єднані");
        assert_eq!(spec["sections"][1]["heading"], "Від'єднані — були під'єднані раніше");
        assert_eq!(spec["sections"][0]["columns"][0], "Категорія");
        assert!(spec["summary"][0].as_str().unwrap().starts_with("Усього пристроїв"));
        assert!(spec["charts"][0]["title"] == "Пристрої за категорією");
    }
}
