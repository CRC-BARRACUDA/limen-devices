//! Windows device enumeration from the registry `Enum` tree — devices ever
//! connected. Live presence is not resolved (all shown as previously seen).

use limen_sdk_rust::{json, Value};
use winreg::enums::HKEY_LOCAL_MACHINE;
use winreg::RegKey;

use crate::device;

pub fn list_devices() -> Value {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let mut devices: Vec<Value> = Vec::new();

    // (registry Enum subkey, category, type). All are two levels deep:
    // Enum\<KEY>\<class-or-vidpid>\<instance>, with a FriendlyName value.
    let sources = [
        ("USB", "usb", "usb"),
        ("USBSTOR", "usb", "flash"),
        ("PCI", "pci", "pci"),
        ("DISPLAY", "monitor", "display"),
        ("SCSI", "disk", "disk"),
        ("IDE", "disk", "disk"),
        ("BTHENUM", "bluetooth", "device"),
    ];

    for (subkey, category, dtype) in sources {
        let path = format!(r"SYSTEM\CurrentControlSet\Enum\{subkey}");
        let Ok(root) = hklm.open_subkey(&path) else {
            continue;
        };
        for group in root.enum_keys().flatten() {
            let Ok(group_key) = root.open_subkey(&group) else {
                continue;
            };
            for instance in group_key.enum_keys().flatten() {
                let inst_key = group_key.open_subkey(&instance).ok();
                let friendly = inst_key
                    .as_ref()
                    .and_then(|k| k.get_value::<String, _>("FriendlyName").ok());
                let mfg = inst_key
                    .as_ref()
                    .and_then(|k| k.get_value::<String, _>("Mfg").ok());
                devices.push(device(
                    category,
                    dtype,
                    group.clone(),
                    mfg,
                    friendly,
                    Some(instance.clone()),
                    false, // registry = history; live presence not resolved here
                ));
            }
        }
    }

    json!({
        "os": "windows",
        "scope": "history",
        "note": "From the Windows registry Enum tree (USB/USBSTOR/PCI/DISPLAY/SCSI/IDE/BTHENUM): \
                 devices ever connected. `connected` is not resolved (all shown as previously seen).",
        "total": devices.len(),
        "connected": 0,
        "was_connected": devices.len(),
        "devices": devices,
    })
}
