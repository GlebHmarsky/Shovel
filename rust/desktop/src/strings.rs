use std::{collections::BTreeMap, sync::OnceLock};
pub fn text(key: &str) -> String {
    static STRINGS: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    let strings = STRINGS.get_or_init(|| {
        serde_json::from_str(include_str!("../resources/strings.json"))
            .expect("checked embedded localization catalogue")
    });
    strings
        .get(key)
        .cloned()
        .unwrap_or_else(|| key.to_owned())
        .replace('_', "")
}
