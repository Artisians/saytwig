//! Central English catalog; UI code uses stable translation keys.
use std::{collections::BTreeMap, sync::OnceLock};
pub(crate) fn t(key: &str) -> &'static str {
    static STRINGS: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    STRINGS
        .get_or_init(|| {
            serde_json::from_str(include_str!("../locales/en.json")).unwrap_or_default()
        })
        .get(key)
        .map(String::as_str)
        .unwrap_or("Translation unavailable")
}
