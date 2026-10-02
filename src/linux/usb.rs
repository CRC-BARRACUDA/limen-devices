//! USB devices: currently attached from `/sys/bus/usb/devices`, plus history
//! parsed from `journalctl -k` (keyed by vid:pid:serial).

use std::collections::BTreeMap;

use limen_sdk_rust::Value;

use super::{read_hex, read_trim};
use crate::device;

#[derive(Default, Clone)]
struct UsbDev {
    vendor_id: String,
    product_id: String,
    product: Option<String>,
    manufacturer: Option<String>,
    serial: Option<String>,
    dtype: Option<String>,
    connected: bool,
    syspath: Option<String>,
}

pub(super) fn collect(out: &mut Vec<Value>) {
    let mut devices: BTreeMap<String, UsbDev> = BTreeMap::new();

    // 1. History from `journalctl -k`.
    parse_journal_history(&mut devices);

    // 2. Currently attached from /sys.
    if let Ok(entries) = std::fs::read_dir("/sys/bus/usb/devices") {
        for entry in entries.flatten() {
            let dir = entry.path();
            let Some(vendor_id) = read_trim(&dir.join("idVendor")) else {
                continue;
            };
            let product_id = read_trim(&dir.join("idProduct")).unwrap_or_default();
            let serial = read_trim(&dir.join("serial"));
            let key = key(&vendor_id, &product_id, &serial);
            let product = read_trim(&dir.join("product"));
            let manufacturer = read_trim(&dir.join("manufacturer"));
            let dtype = classify(&dir);
            let d = devices.entry(key).or_insert_with(|| UsbDev {
                vendor_id,
                product_id,
                serial,
                ..Default::default()
            });
            d.connected = true;
            if d.syspath.is_none() {
                d.syspath = Some(dir.to_string_lossy().into_owned());
            }
            if d.product.is_none() {
                d.product = product;
            }
            if d.manufacturer.is_none() {
                d.manufacturer = manufacturer;
            }
            if d.dtype.is_none() {
                d.dtype = dtype;
            }
        }
    }

    for d in devices.values() {
        out.push(device(
            "usb",
            d.dtype.as_deref().unwrap_or("usb"),
            format!("{}:{}", d.vendor_id, d.product_id),
            d.manufacturer.clone(),
            d.product.clone(),
            d.serial.clone(),
            d.connected,
            d.syspath.clone(),
        ));
    }
}

/// A coarse device type from a USB `(interface_class, protocol)` pair.
fn type_for_class(class: u32, protocol: u32) -> Option<&'static str> {
    Some(match class {
        0x08 => "flash",
        0x03 => match protocol {
            1 => "keyboard",
            2 => "mouse",
            _ => "hid",
        },
        0x0e | 0x06 => "camera",
        0x01 => "audio",
        0x07 => "printer",
        0x02 | 0x0a => "serial",
        0x0b => "smartcard",
        0x09 => "hub",
        0xe0 => "wireless",
        _ => return None,
    })
}

/// Classify a device by looking at its interfaces in sysfs.
fn classify(dir: &std::path::Path) -> Option<String> {
    if read_hex(&dir.join("bDeviceClass")) == Some(0x09) {
        return Some("hub".into());
    }
    let mut hub = false;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if !name.to_string_lossy().contains(':') {
                continue;
            }
            let iface = entry.path();
            let Some(class) = read_hex(&iface.join("bInterfaceClass")) else {
                continue;
            };
            let proto = read_hex(&iface.join("bInterfaceProtocol")).unwrap_or(0);
            match type_for_class(class, proto) {
                Some("hub") => hub = true,
                Some(t) => return Some(t.into()),
                None => {}
            }
        }
    }
    Some(if hub { "hub" } else { "other" }.into())
}

fn key(vid: &str, pid: &str, serial: &Option<String>) -> String {
    format!("{vid}:{pid}:{}", serial.as_deref().unwrap_or(""))
}

/// Parse `journalctl -k` for USB attach events into `devices`.
fn parse_journal_history(devices: &mut BTreeMap<String, UsbDev>) {
    let Ok(out) = std::process::Command::new("journalctl")
        .args(["-k", "--no-pager", "-o", "cat"])
        .output()
    else {
        return;
    };
    let text = String::from_utf8_lossy(&out.stdout);

    let mut cur: BTreeMap<String, UsbDev> = BTreeMap::new();
    let finalize = |d: &UsbDev, out: &mut BTreeMap<String, UsbDev>| {
        if !d.vendor_id.is_empty() {
            out.entry(key(&d.vendor_id, &d.product_id, &d.serial))
                .or_insert_with(|| d.clone());
        }
    };

    for line in text.lines() {
        let Some(rest) = line.strip_prefix("usb ") else {
            continue;
        };
        let Some((port, msg)) = rest.split_once(": ") else {
            continue;
        };
        if msg.starts_with("New USB device found") {
            if let Some(prev) = cur.remove(port) {
                finalize(&prev, devices);
            }
            cur.insert(
                port.to_string(),
                UsbDev {
                    vendor_id: field(msg, "idVendor=").unwrap_or_default(),
                    product_id: field(msg, "idProduct=").unwrap_or_default(),
                    ..Default::default()
                },
            );
        } else if let Some(v) = msg.strip_prefix("Product: ") {
            if let Some(d) = cur.get_mut(port) {
                d.product = Some(v.trim().to_string());
            }
        } else if let Some(v) = msg.strip_prefix("Manufacturer: ") {
            if let Some(d) = cur.get_mut(port) {
                d.manufacturer = Some(v.trim().to_string());
            }
        } else if let Some(v) = msg.strip_prefix("SerialNumber: ") {
            if let Some(d) = cur.get_mut(port) {
                d.serial = Some(v.trim().to_string());
            }
        }
    }
    for (_, d) in cur {
        finalize(&d, devices);
    }
}

/// Extract the alphanumeric value right after `marker` (e.g. `idVendor=`).
fn field(s: &str, marker: &str) -> Option<String> {
    let i = s.find(marker)? + marker.len();
    let rest = &s[i..];
    let end = rest
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(rest.len());
    Some(rest[..end].to_string())
}
