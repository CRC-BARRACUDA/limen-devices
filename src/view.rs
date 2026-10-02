//! Every screen this module draws.

use crate::*;

/// The landing view: nothing is scanned until the user asks. Just a hint and a
/// Scan button that invokes `scan`.
pub(crate) fn idle_view(lang: &str) -> Value {
    let t = |k: &str| catalog().tr(lang, k);
    window(
        t("ui.title"),
        vec![
            label(t("ui.idle_hint")).weak(),
            button(t("ui.scan"), CAP, "scan").primary(),
        ],
    )
}

/// The right-click menu shared by every device row. The activated row's id is
/// added by the host, so each entry only needs its `target`. On Windows a device
/// lives in the Registry or Device Manager — never on the filesystem — so those
/// are the only two destinations; Linux has a single path entry.
pub(crate) fn row_menu(lang: &str) -> Vec<MenuItem> {
    let t = |k: &str| catalog().tr(lang, k);
    let mut items = vec![menu_item(t("menu.about"), CAP, "about").open_in_tab()];
    #[cfg(target_os = "windows")]
    {
        items.push(limen_sdk_rust::ui::submenu(
            t("menu.open_in"),
            vec![
                menu_item(t("menu.registry"), CAP, "open_path")
                    .args(json!({ "target": "registry" })),
                menu_item(t("menu.device_manager"), CAP, "open_path")
                    .args(json!({ "target": "device_manager" })),
            ],
        ));
    }
    #[cfg(not(target_os = "windows"))]
    {
        items.push(
            menu_item(t("menu.open_path"), CAP, "open_path")
                .args(json!({ "target": "path" })),
        );
    }
    items
}

impl Devices {
    /// The results view: a search box + Refresh (+ Make Report when a report
    /// provider is loaded), then two sections (Connected / Disconnected). Filters
    /// `devices` by `query_raw` and caches each shown device by its row id so row
    /// actions (`about` / `open_path`) resolve it.
    pub(crate) fn render(
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
        let mut actions = vec![button(t("ui.refresh"), CAP, "scan").primary()];
        if report {
            actions.push(
                button(t("ui.report"), CAP, "report_config").open_in_tab(),
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
                    .on_activate(CAP, "about"),
                separator(),
                label(t("ui.disconnected").replace("{n}", &disc_rows.len().to_string())).strong(),
                table(cols, disc_rows)
                    .row_ids(disc_ids)
                    .row_menu(menu)
                    .on_activate(CAP, "about"),
            ],
        )
    }

    /// A detail view for one device (opened in a new tab from a row action).
    pub(crate) fn about(&self, params: &Value, lang: &str) -> Value {
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
                button(t("detail.open_registry"), CAP, "open_path")
                    .args(json!({ "id": id, "target": "registry" })),
            );
            widgets.push(
                button(t("detail.open_device_manager"), CAP, "open_path")
                    .args(json!({ "id": id, "target": "device_manager" })),
            );
        }
        #[cfg(not(target_os = "windows"))]
        {
            if !path.is_empty() {
                widgets.push(
                    button(t("detail.open_path"), CAP, "open_path")
                        .args(json!({ "id": id, "target": "path" }))
                        .primary(),
                );
            }
        }

        window(title, widgets)
    }
}
