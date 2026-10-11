//! Texts for people are not built in Rust: the core returns a [`Msg`] – a key
//! and its arguments – and the interface renders it in the user's language.
//! The app translates the key in the frontend (Paraglide); the CLI renders it
//! here, from the same catalogs (`app/messages/{de,en}.json`), so every text
//! exists exactly once per language.
//!
//! Messages used from Rust are plain strings with `{name}` placeholders (no
//! plural variants); `tests/i18n.rs` checks that every key used in Rust exists
//! in both catalogs with the same placeholders.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// A text for people: a catalog key and its arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Msg {
    pub key: String,
    pub args: BTreeMap<String, MsgArg>,
}

/// An argument: plain text (a path, a host, ssh's own output) or another
/// message that is translated as well.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(untagged)]
pub enum MsgArg {
    Text(String),
    Msg(Box<Msg>),
}

impl Msg {
    pub fn new(key: impl Into<String>) -> Self {
        Msg { key: key.into(), args: BTreeMap::new() }
    }

    /// Adds a plain-text argument.
    pub fn arg(mut self, name: &str, value: impl fmt::Display) -> Self {
        self.args.insert(name.to_string(), MsgArg::Text(value.to_string()));
        self
    }

    /// Adds an argument that is itself a message.
    pub fn nested(mut self, name: &str, value: Msg) -> Self {
        self.args.insert(name.to_string(), MsgArg::Msg(Box::new(value)));
        self
    }

    /// The text in `lang`. An unknown key comes back as the key itself.
    pub fn render(&self, lang: Lang) -> String {
        let Some(template) = catalog(lang).get(&self.key) else { return self.key.clone() };
        let mut out = String::with_capacity(template.len());
        let mut rest = template.as_str();
        while let Some(start) = rest.find('{') {
            out.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            match after.find('}') {
                Some(end) if after[..end].chars().all(|c| c.is_ascii_alphanumeric() || c == '_') => {
                    let name = &after[..end];
                    match self.args.get(name) {
                        Some(MsgArg::Text(t)) => out.push_str(t),
                        Some(MsgArg::Msg(m)) => out.push_str(&m.render(lang)),
                        None => out.push_str(&rest[start..start + end + 2]),
                    }
                    rest = &after[end + 1..];
                }
                _ => {
                    out.push('{');
                    rest = after;
                }
            }
        }
        out.push_str(rest);
        out
    }
}

/// Rendered in the process language ([`set_lang`]).
impl fmt::Display for Msg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render(lang()))
    }
}

/// `msg!("doctor_sshDir_ok", mode = "700")` – a [`Msg`] with plain-text arguments.
#[macro_export]
macro_rules! msg {
    ($key:literal $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::i18n::Msg::new($key)$(.arg(stringify!($name), $value))*
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    De,
    En,
}

impl Lang {
    /// From a locale such as `de_DE.UTF-8` or `en-GB`: German for `de*`, English otherwise.
    pub fn from_locale(locale: &str) -> Lang {
        if locale.to_ascii_lowercase().starts_with("de") {
            Lang::De
        } else {
            Lang::En
        }
    }
}

static LANG: AtomicU8 = AtomicU8::new(0);

/// The language [`Msg`]'s `Display` uses (German until set).
pub fn set_lang(lang: Lang) {
    LANG.store(lang as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    if LANG.load(Ordering::Relaxed) == Lang::En as u8 {
        Lang::En
    } else {
        Lang::De
    }
}

const DE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../app/messages/de.json"));
const EN: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../app/messages/en.json"));

/// The plain-string messages of a catalog (plural variants are frontend-only).
pub fn catalog(lang: Lang) -> &'static BTreeMap<String, String> {
    static CATALOGS: OnceLock<[BTreeMap<String, String>; 2]> = OnceLock::new();
    let parse = |text: &str| -> BTreeMap<String, String> {
        let value: serde_json::Value = serde_json::from_str(text).expect("valid message catalog");
        value
            .as_object()
            .map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect())
            .unwrap_or_default()
    };
    &CATALOGS.get_or_init(|| [parse(DE), parse(EN)])[lang as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_arguments_nested_messages_and_unknown_keys() {
        let m = msg!("error_profileNotFound", id = "acme");
        assert_eq!(m.render(Lang::De), "Key „acme“ existiert nicht");
        assert_eq!(m.render(Lang::En), "There is no key “acme”");
        let nested = msg!("error_invalidValue", value = "a b").nested("field", msg!("field_alias"));
        assert_eq!(nested.render(Lang::En), "Invalid value for alias: “a b”");
        assert_eq!(Msg::new("no_such_key").render(Lang::De), "no_such_key");
        // A missing argument stays visible instead of vanishing.
        assert_eq!(Msg::new("error_profileNotFound").render(Lang::En), "There is no key “{id}”");
    }

    #[test]
    fn language_from_locale() {
        assert_eq!(Lang::from_locale("de_DE.UTF-8"), Lang::De);
        assert_eq!(Lang::from_locale("de-AT"), Lang::De);
        assert_eq!(Lang::from_locale("en_US.UTF-8"), Lang::En);
        assert_eq!(Lang::from_locale("fr_FR"), Lang::En);
    }

    #[test]
    fn serializes_as_key_and_args() {
        let m = msg!("error_invalidValue", value = "x").nested("field", msg!("field_alias"));
        let json = serde_json::to_value(&m).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"key": "error_invalidValue", "args": {"field": {"key": "field_alias", "args": {}}, "value": "x"}})
        );
        assert_eq!(serde_json::from_value::<Msg>(json).unwrap(), m);
    }
}
