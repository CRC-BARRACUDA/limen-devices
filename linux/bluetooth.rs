//! Bluetooth adapters from `/sys/class/bluetooth`.

use limen_sdk_rust::Value;

use super::read_trim;
use crate::device;

pub(super) fn collect(out: &mut Vec<Value>) {
    let Ok(entries) = std::fs::read_dir("/sys/class/bluetooth") else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let addr = read_trim(&entry.path().join("address")).unwrap_or_default();
        out.push(device("bluetooth", "adapter", addr, None, Some(name), None, true));
    }
}
