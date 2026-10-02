//! What the module has to keep being.

mod i18n;

use crate::*;

/// One device, as a scan would record it.
pub(crate) fn seen(id: &str, connected: bool) -> Value {
    json!({
        "category": "usb", "type": "storage", "id": id,
        "vendor": "Acme", "product": "Stick", "serial": "SN1",
        "connected": connected, "path": "",
    })
}

pub(crate) fn scanned(devices: Vec<Value>) -> Devices {
    Devices { last_devices: devices, scanned: true, ..Default::default() }
}

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

/// The report is filed under the machine it is about.
///
/// A report is kept — attached to a ticket, filed with a case — and a folder of
/// documents all called "devices" is one nobody can find anything in. The date
/// is the report module's to add.
#[test]
fn the_report_is_named_after_the_machine() {
    let spec = scanned(vec![seen("a", true)]).report_spec("view", "", "", "en");
    let name = spec["file_name"].as_str().expect("a name to save it under");
    assert!(!name.is_empty());
    assert!(name.ends_with("devices"), "{name}");
    // And the same machine is named in the line under the title.
    let subtitle = spec["subtitle"].as_str().unwrap();
    if name != "devices" {
        let host = name.trim_end_matches("_devices");
        assert!(subtitle.starts_with(host), "{subtitle} does not name {host}");
    }
    assert!(subtitle.contains('1'), "the counts are still there: {subtitle}");
}

/// A machine that will not give up its name still produces a report — one whose
/// subtitle does not start with a stray separator.
#[test]
fn a_nameless_machine_still_reports() {
    let line = catalog().tr("en", "report.subtitle")
        .replace("{total}", "2")
        .replace("{connected}", "1");
    assert!(!line.starts_with(['·', ' ']), "{line}");
    assert!(line.contains('2') && line.contains('1'));
}

/// The preview is the only answer this module asks for. What the document
/// becomes is the report module's question, and it owns the controls for it.
#[test]
fn the_dialog_asks_only_what_it_uses() {
    let json = report_config("en").to_string();
    assert!(json.contains("content") && json.contains("scope"));
    assert!(!json.contains("format"), "a format picker came back: {json}");
    let spec = scanned(vec![seen("a", true)]).report_spec("view", "", "", "en");
    assert_eq!(spec["format"], "view");
}
