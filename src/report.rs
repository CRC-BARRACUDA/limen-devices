//! The report: what to ask for, and the spec a report provider is handed.

use crate::*;

/// This machine's name, for the report's subtitle and its filename.
///
/// Read from the kernel on Linux and from the environment on Windows — no new
/// permission, and no process spawned for a string the system already has.
/// Empty rather than a guess if neither answers: a report labelled "unknown" is
/// one somebody has to open to identify.
fn hostname() -> String {
    #[cfg(target_os = "linux")]
    {
        if let Ok(name) = std::fs::read_to_string("/proc/sys/kernel/hostname") {
            let name = name.trim();
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }
    for var in ["COMPUTERNAME", "HOSTNAME"] {
        if let Ok(name) = std::env::var(var) {
            if !name.trim().is_empty() {
                return name.trim().to_string();
            }
        }
    }
    String::new()
}

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
        // Always the preview. What the document is, and the ground it is
        // printed on, are decided there — beside the thing being saved, by the
        // module that owns the question — rather than in a dropdown here that
        // has to be kept in step with what a provider can actually produce.
        let fmt = "view";
        let content = params.get("content").and_then(Value::as_str).unwrap_or("");
        let scope = params.get("scope").and_then(Value::as_str).unwrap_or("");
        let spec = self.report_spec(fmt, content, scope, lang);
        match host.call("report.build", "build", spec) {
            // The report provider returned a view — show it in this tab.
            Ok(v) if v.get("widgets").is_some() => v,
            // A provider that writes a file and acknowledges with nothing. The
            // one shipped with Limen always answers with a screen; this is for
            // any other.
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

        // The machine this is about, in the line under the title and in the
        // name the file is offered under. The report module adds the date; a
        // folder of files all called "devices" is one nobody can find anything
        // in.
        let host = hostname();
        let subtitle = t(if host.is_empty() {
            "report.subtitle"
        } else {
            "report.subtitle_host"
        })
        .replace("{host}", &host)
        .replace("{total}", &total.to_string())
        .replace("{connected}", &connected.to_string());
        let file_name = if host.is_empty() {
            "devices".to_string()
        } else {
            format!("{host}_devices")
        };

        json!({
            "title": t("report.title"),
            "subtitle": subtitle,
            "file_name": file_name,
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
