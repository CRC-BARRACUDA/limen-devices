//! Non-USB block devices from `/sys/block` (USB-backed storage is skipped —
//! it's already reported under `usb`).

use limen_sdk_rust::Value;

use super::read_trim;
use crate::device;

pub(super) fn collect(out: &mut Vec<Value>) {
    let Ok(entries) = std::fs::read_dir("/sys/block") else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // Skip virtual/loop/ram/device-mapper/optical block devices.
        if ["loop", "ram", "zram", "dm-", "md", "sr"]
            .iter()
            .any(|p| name.starts_with(p))
        {
            continue;
        }
        // Skip USB-attached storage — already listed under `usb`.
        let real = std::fs::canonicalize(entry.path()).unwrap_or_else(|_| entry.path());
        if real.to_string_lossy().contains("/usb") {
            continue;
        }
        let dev = entry.path().join("device");
        let model = read_trim(&dev.join("model"));
        let vendor = read_trim(&dev.join("vendor"));
        let serial = read_trim(&dev.join("serial")).or_else(|| read_trim(&dev.join("wwid")));
        let dtype = if name.starts_with("nvme") { "nvme" } else { "disk" };
        let path = Some(entry.path().to_string_lossy().into_owned());
        out.push(device("disk", dtype, name, vendor, model, serial, true, path));
    }
}
