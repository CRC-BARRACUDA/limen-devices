//! What the module has to keep being.

mod i18n;

use crate::*;

#[test]
fn lists_devices_across_categories() {
    let v = list_devices();
    let devs = v.get("devices").and_then(Value::as_array).expect("devices array");
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for d in devs {
        assert!(d.get("category").and_then(Value::as_str).is_some());
        assert!(d.get("type").is_some());
        assert!(d.get("connected").is_some());
        let c = d.get("category").and_then(Value::as_str).unwrap_or("?");
        *counts.entry(c.to_string()).or_default() += 1;
    }
    eprintln!("total = {}, by category = {counts:?}", devs.len());
}
