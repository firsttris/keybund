//! The message catalogs (`app/messages/{de,en}.json`) and the Rust code agree:
//! every key Rust uses exists in both languages as a plain string, with the
//! placeholders the code fills in.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use keybund_core::i18n::{catalog, Lang};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Placeholders of a message text: `{name}`.
fn placeholders(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = text;
    while let Some(i) = rest.find('{') {
        rest = &rest[i + 1..];
        if let Some(j) = rest.find('}') {
            let name = &rest[..j];
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                out.insert(name.to_string());
            }
        }
    }
    out
}

/// The text from `start` up to the matching closing parenthesis.
fn call_body(src: &str, start: usize) -> &str {
    let mut depth = 0;
    let mut in_str = false;
    let bytes = src.as_bytes();
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        match b {
            b'"' if bytes[i - 1] != b'\\' => in_str = !in_str,
            b'(' if !in_str => depth += 1,
            b')' if !in_str => {
                depth -= 1;
                if depth == 0 {
                    return &src[start..=i];
                }
            }
            _ => {}
        }
    }
    &src[start..]
}

/// Every `msg!("key", a = …, b = …)` in the sources, with the argument names
/// (plus arguments added right after with `.nested("name", …)`).
fn uses() -> BTreeMap<String, BTreeSet<String>> {
    let mut files = Vec::new();
    for dir in ["crates/core/src", "crates/cli/src", "app/src-tauri/src"] {
        rust_files(&root().join(dir), &mut files);
    }
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for file in files {
        let src = std::fs::read_to_string(&file).unwrap();
        let mut from = 0;
        while let Some(i) = src[from..].find("msg!(") {
            let start = from + i + "msg!".len();
            from = start;
            let body = call_body(&src, start);
            let inner = body[1..body.len() - 1].trim_start();
            // `msg!(` in docs and the macro itself have no string key.
            let Some(quoted) = inner.strip_prefix('"') else { continue };
            let key = quoted.split('"').next().unwrap().to_string();
            // Macro definitions and docs use placeholders like "key".
            if !key.contains('_') {
                continue;
            }
            let mut args: BTreeSet<String> = inner
                .split(',')
                .skip(1)
                .filter_map(|part| part.split_once('=').map(|(name, _)| name.trim().to_string()))
                .filter(|name| name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !name.is_empty())
                .collect();
            let after = &src[start + body.len()..];
            if let Some(rest) = after.strip_prefix(".nested(\"") {
                args.insert(rest.split('"').next().unwrap().to_string());
            }
            out.entry(key).or_default().extend(args);
        }
    }
    out
}

#[test]
fn every_key_used_in_rust_is_in_both_catalogs_with_its_placeholders() {
    let used = uses();
    assert!(used.len() > 100, "found only {} keys – is the scan broken?", used.len());
    let mut problems = Vec::new();
    for (key, args) in &used {
        for lang in [Lang::De, Lang::En] {
            match catalog(lang).get(key) {
                None => problems.push(format!("{key}: missing in {lang:?} (or not a plain string)")),
                Some(text) => {
                    let missing: Vec<_> = placeholders(text).difference(args).cloned().collect();
                    if !missing.is_empty() {
                        problems.push(format!("{key} ({lang:?}): no argument for {missing:?}"));
                    }
                }
            }
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_field_names_of_invalid_values_exist() {
    for field in ["name", "key", "gitName", "gitEmail", "folder", "hostname", "alias", "user"] {
        for lang in [Lang::De, Lang::En] {
            assert!(catalog(lang).contains_key(&format!("field_{field}")), "field_{field} in {lang:?}");
        }
    }
}

#[test]
fn both_catalogs_have_the_same_keys_and_placeholders() {
    let read = |lang: &str| -> serde_json::Map<String, serde_json::Value> {
        let path = root().join(format!("app/messages/{lang}.json"));
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    };
    let (de, en) = (read("de"), read("en"));
    let keys = |m: &serde_json::Map<String, serde_json::Value>| m.keys().cloned().collect::<BTreeSet<_>>();
    assert_eq!(keys(&de), keys(&en));
    for (key, value) in &de {
        let (a, b) = (placeholders(&value.to_string()), placeholders(&en[key].to_string()));
        assert_eq!(a, b, "{key}: placeholders differ");
    }
}
