//! The merge of manifest tables: a file of the user over a manifest with the same `id`,
//! or over the manifest that it extends.

/// Puts `over` on `base`. Tables merge key by key, and everything else replaces, an array too.
/// So a file that gives `[[detect.process]]` replaces the rules below it, and a file that gives
/// only `[launch] argv` keeps the other keys of `[launch]`.
pub fn deep_merge(base: &mut toml::Table, over: toml::Table) {
    for (key, value) in over {
        match (base.get_mut(&key), value) {
            (Some(toml::Value::Table(into)), toml::Value::Table(from)) => deep_merge(into, from),
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

/// The keys of `given` that the typed manifest does not have, as dotted paths. The typed manifest
/// is `known` after a round trip through the types. A key that a version of neohar does not know
/// is a warning and not an error, so that a file for a newer version still loads.
pub fn unknown_keys(given: &toml::Table, known: &toml::Table) -> Vec<String> {
    let mut out = Vec::new();
    walk("", given, known, &mut out);
    out
}

fn walk(prefix: &str, given: &toml::Table, known: &toml::Table, out: &mut Vec<String>) {
    for (key, value) in given {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match (value, known.get(key)) {
            (_, None) => out.push(path),
            (toml::Value::Table(inner), Some(toml::Value::Table(inner_known))) => {
                walk(&path, inner, inner_known, out);
            }
            (toml::Value::Array(items), Some(toml::Value::Array(items_known))) => {
                for (index, item) in items.iter().enumerate() {
                    if let (toml::Value::Table(inner), Some(toml::Value::Table(inner_known))) =
                        (item, items_known.get(index))
                    {
                        walk(&format!("{path}[{index}]"), inner, inner_known, out);
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(text: &str) -> toml::Table {
        text.parse().expect("a table")
    }

    #[test]
    fn tables_merge_and_arrays_and_values_replace() {
        let mut base = table(
            "id = 'a'\n[launch]\nargv = ['a']\nready = { timeout_ms = 1 }\n[[detect.process]]\nexe_basename = ['a']\n",
        );
        deep_merge(
            &mut base,
            table(
                "[launch]\nargv = ['b']\n[[detect.process]]\nexe_basename = ['b']\n[[detect.process]]\nexe_basename = ['c']\n",
            ),
        );
        assert_eq!(
            base["launch"]["argv"].as_array().expect("an array").len(),
            1
        );
        assert_eq!(base["launch"]["argv"][0].as_str(), Some("b"));
        // The other key of the table stays.
        assert_eq!(base["launch"]["ready"]["timeout_ms"].as_integer(), Some(1));
        // The array of rules is replaced as a whole.
        assert_eq!(
            base["detect"]["process"].as_array().expect("rules").len(),
            2
        );
        assert_eq!(base["id"].as_str(), Some("a"));
    }

    #[test]
    fn the_keys_that_the_types_do_not_have_are_found() {
        let given = table("a = 1\nb = { c = 2, d = 3 }\n[[rules]]\nx = 1\ny = 2\n");
        let known = table("a = 1\nb = { c = 2 }\n[[rules]]\nx = 1\n");
        assert_eq!(unknown_keys(&given, &known), ["b.d", "rules[0].y"]);
        assert!(unknown_keys(&known, &known).is_empty());
    }
}
