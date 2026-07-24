//! Linux device enumeration from `/sys` (+ USB history from the kernel journal).
//!
//! Each bus has its own submodule with a `collect(&mut Vec<Value>)` fn; this
//! module fans out to them and wraps the result in the summary envelope.

use limen_sdk_rust::{json, Value};

mod bluetooth;
mod disk;
mod monitor;
mod net;
mod pci;
mod usb;

pub fn list_devices() -> Value {
    let mut devices: Vec<Value> = Vec::new();
    usb::collect(&mut devices);
    pci::collect(&mut devices);
    monitor::collect(&mut devices);
    disk::collect(&mut devices);
    net::collect(&mut devices);
    bluetooth::collect(&mut devices);

    let connected = devices
        .iter()
        .filter(|d| d.get("connected").and_then(Value::as_bool).unwrap_or(false))
        .count();

    json!({
        "os": "linux",
        "note": "Devices on this machine across buses (usb/pci/monitor/disk/net/bluetooth). \
                 `connected` = present now; disconnected USB entries come from the kernel \
                 journal (limited to its retention).",
        "total": devices.len(),
        "connected": connected,
        "was_connected": devices.len() - connected,
        "devices": devices,
    })
}

// ---- shared sysfs helpers (visible to the child collectors via `super::`) ---- //

/// Read a sysfs attribute, trimmed; `None` if missing or empty.
fn read_trim(path: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Read a hex sysfs attribute (with or without `0x`) as a `u32`.
fn read_hex(path: &std::path::Path) -> Option<u32> {
    read_trim(path).and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
}
