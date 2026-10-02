//! The catalogue, and that both languages actually say everything.

use crate::tests::{scanned, seen};
use crate::*;

/// Every `a.b` key a locale file defines, read from the file rather than
/// through the catalogue: `tr` falls back to English for a key Ukrainian is
/// missing, so asking it would hide exactly what this is looking for.
fn keys(src: &str) -> Vec<String> {
    let mut table = String::new();
    let mut out = Vec::new();
    for line in src.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            table = name.to_string();
        } else if let Some((key, _)) = line.split_once(" = ") {
            out.push(format!("{table}.{key}"));
        }
    }
    out
}

/// Ukrainian says everything English says. A key only one of them has is a
/// screen that falls back to English mid-sentence.
#[test]
fn both_languages_say_the_same_things() {
    let en = keys(include_str!("../../locales/en.toml"));
    let uk = keys(include_str!("../../locales/uk.toml"));
    assert!(en.len() > 40, "the catalogue is suspiciously small");
    for key in &en {
        assert!(uk.contains(key), "uk.toml is missing {key}");
    }
    for key in uk.iter().filter(|k| !k.starts_with("module.")) {
        assert!(en.contains(key), "en.toml is missing {key}");
    }
}

/// A screen asked for in Ukrainian comes back in Ukrainian — not a mix, and
/// not English with a translated title.
#[test]
fn the_screens_are_translated_not_merely_titled() {
    let idle = idle_view("uk").to_string();
    assert!(idle.contains("Сканувати"), "{idle}");
    assert!(!idle.contains("Scan this machine"), "English survived: {idle}");

    let cfg = report_config("uk").to_string();
    for word in ["Параметри звіту", "Лише таблиці", "Лише під'єднані", "Створити"] {
        assert!(cfg.contains(word), "{word} is missing from {cfg}");
    }
    assert!(!cfg.contains("Tables only"), "English survived: {cfg}");

    let mut m = scanned(vec![seen("a", true)]);
    m.last.insert("0".into(), seen("a", true));
    let about = m.about(&json!({ "id": "0" }), "uk").to_string();
    for word in ["Категорія", "Виробник", "Під'єднано"] {
        assert!(about.contains(word), "{word} is missing from {about}");
    }

    let table = m.render(&[seen("a", true)], "", false, "uk").to_string();
    assert!(table.contains("Серійний номер"), "the columns: {table}");
    assert!(!table.contains("\"Serial\""), "English survived: {table}");
}

/// A dropdown's answer comes back in the language it was shown in, so the
/// module has to recognise its own words — in either language, because a
/// spec built elsewhere may still say "Connected only".
#[test]
fn a_choice_is_understood_in_the_language_it_was_made_in() {
    assert!(chose("Лише під'єднані", "report.scope_connected"));
    assert!(chose("Connected only", "report.scope_connected"));
    assert!(!chose("Лише від'єднані", "report.scope_connected"));

    let m = scanned(vec![seen("on", true), seen("off", false)]);
    for answer in ["Лише під'єднані", "Connected only"] {
        let spec = m.report_spec("view", "", answer, "uk");
        let sections = spec["sections"].as_array().unwrap();
        assert_eq!(sections.len(), 1, "{answer} did not narrow the report");
        assert_eq!(sections[0]["rows"].as_array().unwrap().len(), 1);
    }
}

/// The report a Ukrainian screen asks for is a Ukrainian report: its title,
/// its headings and its columns, not only the rows it carries.
#[test]
fn the_report_speaks_the_language_it_was_asked_in() {
    let spec = scanned(vec![seen("a", true), seen("b", false)])
        .report_spec("view", "", "", "uk");
    assert_eq!(spec["title"], "Звіт про пристрої");
    assert_eq!(spec["sections"][0]["heading"], "Під'єднані");
    assert_eq!(spec["sections"][1]["heading"], "Від'єднані — були під'єднані раніше");
    assert_eq!(spec["sections"][0]["columns"][0], "Категорія");
    assert!(spec["summary"][0].as_str().unwrap().starts_with("Усього пристроїв"));
    assert!(spec["charts"][0]["title"] == "Пристрої за категорією");
}
