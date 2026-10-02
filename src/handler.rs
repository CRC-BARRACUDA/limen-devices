//! What the module is while it runs, and every method it answers to.

use crate::*;

#[derive(Default)]
pub(crate) struct Devices {
    /// Whether the user has scanned this session. Once true, reopening the tab
    /// shows the saved results instead of the landing Scan button.
    pub(crate) scanned: bool,
    /// The raw search text from the last scan (restored into the search box).
    pub(crate) last_query: String,
    /// The full device list from the last scan, so the view can be re-rendered
    /// on reopen without re-enumerating the machine.
    pub(crate) last_devices: Vec<Value>,
    /// The last scan, keyed by the row id sent back on a row action, so
    /// `about` / `open_path` can resolve which device the user acted on.
    pub(crate) last: HashMap<String, Value>,
}

impl Handler for Devices {
    fn capabilities(&self) -> Vec<String> {
        vec![CAP.into()]
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
