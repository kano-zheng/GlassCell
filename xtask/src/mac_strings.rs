//! The Mac app's String Catalog from the egui translations.
//!
//! `cargo xtask mac-strings`          write `macos/GridCraft/Resources/Localizable.xcstrings`
//! `cargo xtask mac-strings --check`  fail if it differs from what the translations give
//!
//! The translations are `"English" => "translation",` arms in `crates/ui-egui/src/i18n/*.rs`
//! (Japanese: the `("English", "translation")` table in `i18n.rs`). Keys with a `context|` prefix
//! are egui's way of telling two uses of a word apart; the Mac labels have none, so they're left out.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value, json};

/// (file, String Catalog language code).
const LANGUAGES: &[(&str, &str)] = &[
    ("cs.rs", "cs"),
    ("de.rs", "de"),
    ("es.rs", "es"),
    ("fr.rs", "fr"),
    ("ko.rs", "ko"),
    ("pt.rs", "pt-BR"),
    ("ru.rs", "ru"),
    ("sr.rs", "sr"),
    ("zh.rs", "zh-Hans"),
];

const OUTPUT: &str = "macos/GridCraft/Resources/Localizable.xcstrings";

/// Reads the Rust string literal at the start of `s` (after optional spaces): its value and the rest.
fn literal(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    let mut chars = s.char_indices();
    if chars.next()?.1 != '"' {
        return None;
    }
    let mut out = String::new();
    let mut escaped = false;
    for (i, c) in chars {
        if escaped {
            out.push(match c {
                'n' => '\n',
                't' => '\t',
                other => other,
            });
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            return Some((out, s.get(i + 1..)?));
        } else {
            out.push(c);
        }
    }
    None
}

/// The `"a" => "b"` arms of a translation file.
fn match_arms(src: &str) -> Vec<(String, String)> {
    src.lines()
        .filter_map(|line| {
            let (key, rest) = literal(line)?;
            let rest = rest.trim_start().strip_prefix("=>")?;
            let (value, _) = literal(rest)?;
            Some((key, value))
        })
        .collect()
}

/// The `("a", "b"),` rows of the Japanese table in `i18n.rs`.
fn japanese(src: &str) -> Vec<(String, String)> {
    let Some(start) = src.find("const JAPANESE") else { return vec![] };
    src.get(start..)
        .unwrap_or("")
        .lines()
        .skip(1)
        .take_while(|l| !l.trim_start().starts_with("];"))
        .filter_map(|line| {
            let (key, rest) = literal(line.trim_start().strip_prefix('(')?)?;
            let (value, _) = literal(rest.trim_start().strip_prefix(',')?)?;
            Some((key, value))
        })
        .collect()
}

fn catalog(root: &Path) -> Result<String, String> {
    let dir = root.join("crates/ui-egui/src/i18n");
    let mut table: BTreeMap<String, BTreeMap<&str, String>> = BTreeMap::new();
    let mut add = |lang: &'static str, pairs: Vec<(String, String)>| {
        for (key, value) in pairs {
            if key.contains('|') || key.is_empty() || value.is_empty() {
                continue;
            }
            table.entry(key).or_default().insert(lang, value);
        }
    };
    for (file, lang) in LANGUAGES {
        let src = fs::read_to_string(dir.join(file)).map_err(|e| format!("{file}: {e}"))?;
        add(lang, match_arms(&src));
    }
    let i18n = fs::read_to_string(root.join("crates/ui-egui/src/i18n.rs")).map_err(|e| format!("i18n.rs: {e}"))?;
    add("ja", japanese(&i18n));
    let mut strings = Map::new();
    for (key, langs) in table {
        let mut localizations = Map::new();
        for (lang, value) in langs {
            localizations.insert(lang.to_string(), json!({"stringUnit": {"state": "translated", "value": value}}));
        }
        strings.insert(key, json!({"localizations": localizations}));
    }
    let doc = json!({"sourceLanguage": "en", "strings": Value::Object(strings), "version": "1.0"});
    serde_json::to_string_pretty(&doc).map(|s| s + "\n").map_err(|e| e.to_string())
}

pub fn run(root: &Path, args: &[&str]) -> Result<(), String> {
    let text = catalog(root)?;
    let path = root.join(OUTPUT);
    if args.contains(&"--check") {
        let current = fs::read_to_string(&path).unwrap_or_default();
        return if current == text { Ok(()) } else { Err(format!("{OUTPUT} is out of date: run `cargo xtask mac-strings`")) };
    }
    fs::write(&path, text).map_err(|e| format!("{OUTPUT}: {e}"))?;
    println!("wrote {OUTPUT}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_and_arms_parse() {
        assert_eq!(literal(r#"  "a \"b\"\n" => x"#).map(|(v, r)| (v, r.to_string())), Some(("a \"b\"\n".into(), " => x".into())));
        assert_eq!(match_arms("        \"Paste\" => \"粘贴\",\n        _ => return None,"), vec![("Paste".into(), "粘贴".into())]);
        assert_eq!(japanese("const JAPANESE: &[(&str, &str)] = &[\n    (\"Paste\", \"貼り付け\"),\n];"), vec![("Paste".into(), "貼り付け".into())]);
    }
}
