//! Network interfaces from `/sys/class/net` (ethernet / wifi / virtual), with
//! MAC address and link state.

use limen_sdk_rust::Value;

use super::read_trim;
use crate::device;

pub(super) fn collect(out: &mut Vec<Value>) {
    let Ok(entries) = std::fs::read_dir("/sys/class/net") else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "lo" {
            continue;
        }
        let dir = entry.path();
        let mac = read_trim(&dir.join("address")).unwrap_or_default();
        let operstate = read_trim(&dir.join("operstate"));
        let connected = operstate.as_deref() == Some("up");
        let dtype = if dir.join("wireless").exists() {
            "wifi"
        } else if std::fs::read_link(dir.join("device")).is_err() {
            "virtual"
        } else {
            "ethernet"
        };
        out.push(device("net", dtype, mac, None, Some(name), operstate, connected));
    }
}
