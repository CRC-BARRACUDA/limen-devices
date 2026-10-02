//! PCI/PCIe functions from `/sys/bus/pci/devices`, typed by the class byte.

use limen_sdk_rust::Value;

use super::read_hex;
use crate::device;

pub(super) fn collect(out: &mut Vec<Value>) {
    let Ok(entries) = std::fs::read_dir("/sys/bus/pci/devices") else {
        return;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        let vendor = read_hex(&dir.join("vendor")).unwrap_or(0);
        let dev_id = read_hex(&dir.join("device")).unwrap_or(0);
        let class = read_hex(&dir.join("class")).unwrap_or(0);
        let slot = entry.file_name().to_string_lossy().to_string();
        // The PCI class byte (bits 16-23) gives the broad function.
        let dtype = match (class >> 16) & 0xff {
            0x01 => "storage-controller",
            0x02 => "network",
            0x03 => "gpu",
            0x04 => "multimedia",
            0x06 => "bridge",
            0x07 => "communication",
            0x08 => "system",
            0x09 => "input",
            0x0c => "serial-bus",
            0x0d => "wireless",
            _ => "pci",
        };
        let driver = std::fs::read_link(dir.join("driver"))
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()));
        let path = Some(dir.to_string_lossy().into_owned());
        out.push(device(
            "pci",
            dtype,
            format!("{vendor:04x}:{dev_id:04x}"),
            None,
            driver.map(|d| format!("driver: {d}")).or(Some(slot)),
            None,
            true,
            path,
        ));
    }
}
