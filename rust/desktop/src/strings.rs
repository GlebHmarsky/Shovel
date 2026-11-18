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
pub fn format(key: &str, values: &[&str]) -> String {
    format_template(&text(key), values)
}
fn format_template(template: &str, values: &[&str]) -> String {
    let mut result = String::new();
    let mut remaining = template;
    while let Some(start) = remaining.find(['{', '}']) {
        result.push_str(&remaining[..start]);
        remaining = &remaining[start..];
        if remaining.starts_with("{{") {
            result.push('{');
            remaining = &remaining[2..];
            continue;
        }
        if remaining.starts_with('}') {
            result.push('}');
            remaining = &remaining[if remaining.starts_with("}}") { 2 } else { 1 }..];
            continue;
        }
        let Some(end) = remaining.find('}') else {
            break;
        };
        let (field, spec) = remaining[1..end]
            .split_once(':')
            .unwrap_or((&remaining[1..end], ""));
        if let Some(value) = field
            .parse::<usize>()
            .ok()
            .and_then(|index| values.get(index))
        {
            if !spec.is_empty()
                && let Some(formatted) = crate::numbers::format(value, spec)
            {
                result.push_str(&formatted);
            } else {
                result.push_str(value);
            }
        } else {
            result.push_str(&remaining[..=end]);
        }
        remaining = &remaining[end + 1..];
    }
    result.push_str(remaining);
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inserted_git_text_is_never_reinterpreted_as_a_format_string() {
        assert_eq!(
            format_template("{0}: {1:N0}", &["{1:N0}", "42"]),
            "{1:N0}: 42"
        );
        assert_eq!(format_template("{0}", &["{0:N0}"]), "{0:N0}");
        assert_eq!(format_template("{{{0}}}", &["{1}}"]), "{{1}}}");
    }
}
