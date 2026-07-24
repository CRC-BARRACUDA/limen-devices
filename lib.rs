//! `local-devices` — a native Limen module that lists devices connected to
//! **this** machine.
//!
//! Provides `devices.local` with one method, `list`. Implementation is per-OS
//! (compile-time `cfg`):
//!
//! * **Windows** — reads the registry (`…\Enum\USB` + `USBSTOR`), which persists
//!   *every* device ever connected — the classic "USB history".
//! * **Linux** — reads `/sys/bus/usb/devices/`, i.e. what's *currently* attached
//!   (Linux keeps no persistent all-time registry; that limitation is noted in
//!   the result).
//!
//! Built as a native (`cdylib`) module using `limen-sdk-rust`.

use limen_sdk_rust::ui::{button, label, separator, table, text, window};
use limen_sdk_rust::{export_module, json, rpc, Handler, Host, RpcError, Value};

#[derive(Default)]
struct LocalDevices;

impl Handler for LocalDevices {
    fn capabilities(&self) -> Vec<String> {
        vec!["devices.local".into()]
    }

    fn invoke(
        &mut self,
        _capability: &str,
        method: &str,
        params: Value,
        _host: &Host,
    ) -> Result<Value, RpcError> {
        match method {
            "ui" => Ok(ui_spec(&params)),
            "list" => Ok(list_devices()),
            other => Err(RpcError::new(
                rpc::METHOD_NOT_FOUND,
                format!("local-devices has no method {other}"),
            )),
        }
    }
}

/// The module's self-drawn UI: a search box + Refresh, then two sections
/// (Connected / Disconnected). `params.query` filters the list; Refresh re-runs.
fn ui_spec(params: &Value) -> Value {
    let query = params
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase();

    let data = list_devices();
    let empty = Vec::new();
    let devices = data.get("devices").and_then(Value::as_array).unwrap_or(&empty);

    let cell = |d: &Value, key: &str| -> String {
        d.get(key).and_then(Value::as_str).unwrap_or("").to_string()
    };
    let dtype = |d: &Value| d.get("type").and_then(Value::as_str).unwrap_or("—").to_string();
    // Case-insensitive substring match across all fields.
    let matches = |d: &Value| -> bool {
        if query.is_empty() {
            return true;
        }
        let hay = format!(
            "{} {}:{} {} {} {}",
            dtype(d),
            cell(d, "vendor_id"),
            cell(d, "product_id"),
            cell(d, "manufacturer"),
            cell(d, "product"),
            cell(d, "serial"),
        )
        .to_lowercase();
        hay.contains(&query)
    };
    let rows_for = |connected: bool| -> Vec<Vec<String>> {
        devices
            .iter()
            .filter(|d| d.get("connected").and_then(Value::as_bool).unwrap_or(false) == connected)
            .filter(|d| matches(d))
            .map(|d| {
                vec![
                    dtype(d),
                    format!("{}:{}", cell(d, "vendor_id"), cell(d, "product_id")),
                    cell(d, "manufacturer"),
                    cell(d, "product"),
                    cell(d, "serial"),
                ]
            })
            .collect()
    };

    let cols: Vec<String> = ["Type", "VID:PID", "Manufacturer", "Product", "Serial"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let connected = rows_for(true);
    let disconnected = rows_for(false);

    window(
        "Local Devices",
        vec![
            text("query")
                .label("Search")
                .placeholder("type, vendor, product, serial…")
                .default(query.clone()),
            button("Refresh", "devices.local", "ui").primary(),
            separator(),
            label(format!("Connected ({})", connected.len())).strong(),
            table(cols.clone(), connected),
            separator(),
            label(format!("Disconnected — previously connected ({})", disconnected.len())).strong(),
            table(cols, disconnected),
        ],
    )
}

// --------------------------------------------------------------------------- //
// Linux — currently-attached USB devices from sysfs.
// --------------------------------------------------------------------------- //

#[cfg(target_os = "linux")]
#[derive(Default, Clone)]
struct Dev {
    vendor_id: String,
    product_id: String,
    product: Option<String>,
    manufacturer: Option<String>,
    serial: Option<String>,
    /// Device type inferred from USB class (flash / keyboard / mouse / …).
    /// `None` for history-only devices whose interfaces we can't inspect.
    dtype: Option<String>,
    connected: bool,
}

/// A coarse device type from a USB `(interface_class, protocol)` pair.
#[cfg(target_os = "linux")]
fn type_for_class(class: u32, protocol: u32) -> Option<&'static str> {
    Some(match class {
        0x08 => "flash",                 // mass storage
        0x03 => match protocol {         // HID
            1 => "keyboard",
            2 => "mouse",
            _ => "hid",
        },
        0x0e | 0x06 => "camera",         // video / still image
        0x01 => "audio",
        0x07 => "printer",
        0x02 | 0x0a => "serial",         // communications / CDC-data
        0x0b => "smartcard",             // e.g. security keys
        0x09 => "hub",
        0xe0 => "wireless",
        _ => return None,
    })
}

/// Classify a device by looking at its interfaces in sysfs.
#[cfg(target_os = "linux")]
fn classify(dir: &std::path::Path) -> Option<String> {
    // A hub declares itself at the device level.
    if read_hex(&dir.join("bDeviceClass")) == Some(0x09) {
        return Some("hub".into());
    }
    // Otherwise the type lives on the interfaces (dirs like "1-1:1.0").
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
                Some(t) => return Some(t.into()), // first specific type wins
                None => {}
            }
        }
    }
    if hub {
        Some("hub".into())
    } else {
        Some("other".into())
    }
}

#[cfg(target_os = "linux")]
fn read_hex(path: &std::path::Path) -> Option<u32> {
    read_trim(path).and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
}

#[cfg(target_os = "linux")]
fn dev_key(vid: &str, pid: &str, serial: &Option<String>) -> String {
    format!("{vid}:{pid}:{}", serial.as_deref().unwrap_or(""))
}

#[cfg(target_os = "linux")]
fn list_devices() -> Value {
    use std::collections::BTreeMap;
    let mut devices: BTreeMap<String, Dev> = BTreeMap::new();

    // 1. History: every device the kernel has ever seen (within journal
    //    retention), parsed from `journalctl -k`.
    parse_journal_history(&mut devices);

    // 2. Currently attached: from /sys — mark those connected, and add any that
    //    aren't in the journal.
    if let Ok(entries) = std::fs::read_dir("/sys/bus/usb/devices") {
        for entry in entries.flatten() {
            let dir = entry.path();
            // Only real devices expose idVendor (interfaces like "1-1:1.0" don't).
            let Some(vendor_id) = read_trim(&dir.join("idVendor")) else {
                continue;
            };
            let product_id = read_trim(&dir.join("idProduct")).unwrap_or_default();
            let serial = read_trim(&dir.join("serial"));
            let key = dev_key(&vendor_id, &product_id, &serial);
            let product = read_trim(&dir.join("product"));
            let manufacturer = read_trim(&dir.join("manufacturer"));
            let dtype = classify(&dir);
            let d = devices.entry(key).or_insert_with(|| Dev {
                vendor_id,
                product_id,
                serial,
                ..Default::default()
            });
            d.connected = true;
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

    let connected = devices.values().filter(|d| d.connected).count();
    let list: Vec<Value> = devices
        .values()
        .map(|d| {
            json!({
                "vendor_id": d.vendor_id,
                "product_id": d.product_id,
                "type": d.dtype,
                "product": d.product,
                "manufacturer": d.manufacturer,
                "serial": d.serial,
                "connected": d.connected,
            })
        })
        .collect();

    json!({
        "os": "linux",
        "note": "All USB devices seen on this machine. `connected` = currently attached (/sys); \
                 the rest were connected before (from the kernel journal, limited to its retention).",
        "total": list.len(),
        "connected": connected,
        "was_connected": list.len() - connected,
        "devices": list,
    })
}

/// Parse `journalctl -k` for USB attach events into `devices` (keyed by
/// vid:pid:serial), building the historical set.
#[cfg(target_os = "linux")]
fn parse_journal_history(devices: &mut std::collections::BTreeMap<String, Dev>) {
    use std::collections::BTreeMap;
    let Ok(out) = std::process::Command::new("journalctl")
        .args(["-k", "--no-pager", "-o", "cat"])
        .output()
    else {
        return;
    };
    let text = String::from_utf8_lossy(&out.stdout);

    // A device's fields span consecutive lines sharing a "usb <port>:" prefix,
    // starting at "New USB device found". Track a current record per port.
    let mut cur: BTreeMap<String, Dev> = BTreeMap::new();
    let finalize = |d: &Dev, out: &mut std::collections::BTreeMap<String, Dev>| {
        if !d.vendor_id.is_empty() {
            out.entry(dev_key(&d.vendor_id, &d.product_id, &d.serial))
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
                Dev {
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
#[cfg(target_os = "linux")]
fn field(s: &str, marker: &str) -> Option<String> {
    let i = s.find(marker)? + marker.len();
    let rest = &s[i..];
    let end = rest
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

#[cfg(target_os = "linux")]
fn read_trim(path: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

// --------------------------------------------------------------------------- //
// Windows — every device ever connected, from the registry.
// --------------------------------------------------------------------------- //

#[cfg(target_os = "windows")]
fn list_devices() -> Value {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let mut devices = Vec::new();

    // USBSTOR — mass-storage devices ever connected.
    if let Ok(usbstor) = hklm.open_subkey(r"SYSTEM\CurrentControlSet\Enum\USBSTOR") {
        for class in usbstor.enum_keys().flatten() {
            if let Ok(class_key) = usbstor.open_subkey(&class) {
                for instance in class_key.enum_keys().flatten() {
                    let friendly = class_key
                        .open_subkey(&instance)
                        .ok()
                        .and_then(|k| k.get_value::<String, _>("FriendlyName").ok())
                        .unwrap_or_default();
                    devices.push(json!({
                        "kind": "usb-storage",
                        "type": "flash",
                        "class": class,
                        "instance": instance,
                        "friendly_name": friendly,
                        "connected": Value::Null, // registry = history; current state TBD
                    }));
                }
            }
        }
    }

    // USB — every USB device the system has enumerated.
    if let Ok(usb) = hklm.open_subkey(r"SYSTEM\CurrentControlSet\Enum\USB") {
        for vidpid in usb.enum_keys().flatten() {
            if let Ok(vp_key) = usb.open_subkey(&vidpid) {
                for instance in vp_key.enum_keys().flatten() {
                    let friendly = vp_key
                        .open_subkey(&instance)
                        .ok()
                        .and_then(|k| k.get_value::<String, _>("FriendlyName").ok())
                        .unwrap_or_default();
                    devices.push(json!({
                        "kind": "usb",
                        "type": Value::Null,
                        "id": vidpid,
                        "instance": instance,
                        "friendly_name": friendly,
                        "connected": Value::Null, // registry = history; current state TBD
                    }));
                }
            }
        }
    }

    json!({
        "os": "windows",
        "scope": "history",
        "note": "From the Windows registry (Enum\\USB + USBSTOR): all devices ever connected.",
        "count": devices.len(),
        "devices": devices,
    })
}

// --------------------------------------------------------------------------- //
// Other platforms.
// --------------------------------------------------------------------------- //

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn list_devices() -> Value {
    json!({
        "os": std::env::consts::OS,
        "note": "device listing is only implemented for Windows and Linux",
        "devices": [],
    })
}

export_module!(LocalDevices);
