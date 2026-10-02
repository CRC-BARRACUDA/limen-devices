//! The report: what to ask for, and the spec a report provider is handed.

use crate::*;

/// The "Make Report" configuration view (opened in a tab): choose the output,
/// what to include, and which devices, then Generate.
pub(crate) fn report_config(lang: &str) -> Value {
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
            button(t("report.generate"), CAP, "make_report").primary(),
        ],
    )
}

impl Devices {
    /// Build a report spec from the last scan and hand it to a report provider.
    /// `params` come from the config view's selects (`format`/`content`/`scope`).
    pub(crate) fn make_report(&self, params: &Value, host: &Host, lang: &str) -> Value {
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
    pub(crate) fn report_spec(&self, fmt: &str, content: &str, scope: &str, lang: &str) -> Value {
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
}
