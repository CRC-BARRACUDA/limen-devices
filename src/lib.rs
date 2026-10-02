//! `devices` — a native Limen module that lists devices on **this**
//! machine across several buses.
//!
//! Provides `devices.local`. Methods: `list` (raw data), plus `ui`/`scan` for
//! the built-in view — the UI does **not** scan on open; it enumerates only when
//! the user presses **Scan**. Categories: **usb**, **pci**,
//! **monitor** (EDID), **disk** (non-USB), **net**, **bluetooth**. Enumeration is
//! per-OS; the platform code lives in [`linux`] / [`windows`], and every device is
//! emitted in one shared schema (see [`device`]):
//!
//! `category`, `type`, `id`, `vendor`, `product`, `serial`, `connected`.
//!
//! Built as a native (`cdylib`) module using `limen-sdk-rust`.

pub(crate) use std::collections::HashMap;

pub(crate) use limen_sdk_rust::ui::{
    button, label, menu_item, row, select, separator, table, text, window, MenuItem,
};
pub(crate) use limen_sdk_rust::{export_module, json, rpc, Catalog, Handler, Host, RpcError, Value};

/// The capability this module provides. Named once: it is also the target every
/// button on every screen calls back into.
pub(crate) const CAP: &str = "devices.local";

/// Every word this module shows, in each language it has.
///
/// English lives in a file beside the Ukrainian rather than in the code: a
/// string written into the source is a string nobody can translate.
fn catalog() -> &'static Catalog {
    static C: std::sync::OnceLock<Catalog> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        Catalog::new(&[
            ("en", include_str!("../locales/en.toml")),
            ("uk", include_str!("../locales/uk.toml")),
        ])
    })
}

/// Whether a dropdown's answer is this choice, in whichever language it was
/// shown in.
///
/// An option is its own value — a person reading Ukrainian sends Ukrainian
/// back — so the answer is compared against every language rather than against
/// the English it used to be.
fn chose(answer: &str, key: &str) -> bool {
    ["en", "uk"].iter().any(|lang| catalog().tr(lang, key) == answer)
}

/// The six visible columns, as catalogue keys — named once, so the table and
/// the report cannot drift apart.
const COLUMNS: [&str; 6] = [
    "col.category",
    "col.type",
    "col.id",
    "col.vendor",
    "col.product",
    "col.serial",
];

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux::list_devices;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows::list_devices;

/// Fallback for platforms without a collector.
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn list_devices() -> Value {
    json!({
        "os": std::env::consts::OS,
        "note": "device listing is only implemented for Windows and Linux",
        "devices": [],
    })
}

mod entry;
mod handler;
mod report;
mod view;

#[cfg(test)]
mod tests;

// This reads best as one namespace: each part takes `use crate::*` and finds
// everything, rather than every file carrying a list of its neighbours that has
// to be maintained by hand.
pub(crate) use entry::*;
pub(crate) use handler::*;
pub(crate) use report::*;
pub(crate) use view::*;

export_module!(Devices);
