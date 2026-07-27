//! Windows device enumeration from the registry `Enum` tree — every device ever
//! connected — cross-referenced with SetupAPI for which are present right now.

use std::collections::HashSet;

use limen_sdk_rust::{json, Value};
use winreg::enums::HKEY_LOCAL_MACHINE;
use winreg::RegKey;

use crate::device;

/// Resolve an INF-indirect display string.
///
/// `Mfg` and `FriendlyName` are often stored as a reference into an INF file
/// rather than plain text, in the form `@pci.inf,%gendev_mfg%;(Standard system
/// devices)`. Windows caches the already-localized text after the `;`, so the
/// tail is the human-readable name — without this, the raw reference leaks into
/// the UI as a "vendor". Values that aren't references are returned unchanged.
fn inf_string(raw: String) -> String {
    if !raw.starts_with('@') {
        return raw;
    }
    match raw.split_once(';') {
        Some((_, text)) if !text.trim().is_empty() => text.trim().to_string(),
        // A reference with no cached text is worse than nothing — it would show
        // as gibberish, so report it as unknown instead.
        _ => String::new(),
    }
}

/// Instance ids of the devices currently present, upper-cased for comparison.
///
/// The registry `Enum` tree is history: it keeps a key for everything ever
/// plugged in, with no indication of what is attached now. SetupAPI's
/// `DIGCF_PRESENT` is the authoritative live view, so the two are matched by
/// device instance id.
fn present_instance_ids() -> HashSet<String> {
    use std::ffi::c_void;

    #[repr(C)]
    struct SpDevinfoData {
        cb_size: u32,
        class_guid: [u8; 16],
        dev_inst: u32,
        reserved: usize,
    }

    #[link(name = "setupapi")]
    unsafe extern "system" {
        fn SetupDiGetClassDevsW(
            class_guid: *const c_void,
            enumerator: *const u16,
            hwnd: *mut c_void,
            flags: u32,
        ) -> *mut c_void;
        fn SetupDiEnumDeviceInfo(set: *mut c_void, index: u32, data: *mut SpDevinfoData) -> i32;
        fn SetupDiGetDeviceInstanceIdW(
            set: *mut c_void,
            data: *mut SpDevinfoData,
            buf: *mut u16,
            buf_len: u32,
            required: *mut u32,
        ) -> i32;
        fn SetupDiDestroyDeviceInfoList(set: *mut c_void) -> i32;
    }

    const DIGCF_PRESENT: u32 = 0x0000_0002;
    const DIGCF_ALLCLASSES: u32 = 0x0000_0004;

    let mut ids = HashSet::new();
    // SAFETY: a null class GUID + null enumerator asks for every class; the
    // returned handle is checked against INVALID_HANDLE_VALUE and destroyed on
    // every exit path below.
    unsafe {
        let set = SetupDiGetClassDevsW(
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            DIGCF_PRESENT | DIGCF_ALLCLASSES,
        );
        if set.is_null() || set as isize == -1 {
            return ids;
        }
        let mut index = 0u32;
        loop {
            let mut data = SpDevinfoData {
                cb_size: std::mem::size_of::<SpDevinfoData>() as u32,
                class_guid: [0; 16],
                dev_inst: 0,
                reserved: 0,
            };
            if SetupDiEnumDeviceInfo(set, index, &mut data) == 0 {
                break; // no more devices
            }
            index += 1;
            // Instance ids max out well under this; a truncated read is skipped.
            let mut buf = [0u16; 512];
            let mut needed = 0u32;
            if SetupDiGetDeviceInstanceIdW(
                set,
                &mut data,
                buf.as_mut_ptr(),
                buf.len() as u32,
                &mut needed,
            ) != 0
            {
                let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                ids.insert(String::from_utf16_lossy(&buf[..len]).to_uppercase());
            }
        }
        SetupDiDestroyDeviceInfoList(set);
    }
    ids
}

pub fn list_devices() -> Value {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let present = present_instance_ids();
    let mut devices: Vec<Value> = Vec::new();
    let mut connected_count = 0usize;

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
                let value = |name: &str| {
                    inst_key
                        .as_ref()
                        .and_then(|k| k.get_value::<String, _>(name).ok())
                        .map(inf_string)
                        .filter(|s| !s.is_empty())
                };
                // FriendlyName is often absent; DeviceDesc is the fallback label
                // Device Manager itself shows.
                let friendly = value("FriendlyName").or_else(|| value("DeviceDesc"));
                let mfg = value("Mfg");
                // The device's registry key — what the "Registry" row action
                // navigates regedit to.
                let reg_key =
                    format!(r"Computer\HKEY_LOCAL_MACHINE\{path}\{group}\{instance}");
                // The device instance id, as SetupAPI and Device Manager know it.
                let instance_id = format!(r"{subkey}\{group}\{instance}");
                let connected = present.contains(&instance_id.to_uppercase());
                if connected {
                    connected_count += 1;
                }
                let mut dev = device(
                    category,
                    dtype,
                    group.clone(),
                    mfg,
                    friendly,
                    Some(instance.clone()),
                    connected,
                    Some(reg_key),
                );
                // Carried so "Device Manager" can open this exact device.
                dev["instance_id"] = json!(instance_id);
                devices.push(dev);
            }
        }
    }

    let total = devices.len();
    json!({
        "os": "windows",
        "scope": "history",
        "note": "From the Windows registry Enum tree (USB/USBSTOR/PCI/DISPLAY/SCSI/IDE/BTHENUM): \
                 devices ever connected. `connected` is resolved against SetupAPI's list of \
                 devices present right now.",
        "total": total,
        "connected": connected_count,
        "was_connected": total - connected_count,
        "devices": devices,
    })
}
