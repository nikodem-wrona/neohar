//! The model catalog of Codex: the file `models_cache.json` in `$CODEX_HOME`, which Codex
//! writes for its own model picker [verify]. Each model has a slug, a name, a visibility, a
//! priority and the effort levels that it supports. The form of a task lists the models that the
//! picker of Codex lists.

use nh_proto::api::calls::agent::ModelChoice;
use serde::Deserialize;

/// The name of the catalog in the directory of Codex.
pub const CATALOG_FILE: &str = "models_cache.json";

#[derive(Deserialize)]
struct File {
    #[serde(default)]
    models: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    slug: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    visibility: Option<String>,
    #[serde(default)]
    priority: Option<i64>,
    #[serde(default)]
    supported_reasoning_levels: Vec<Level>,
}

#[derive(Deserialize)]
struct Level {
    effort: String,
}

/// The models of the catalog that the picker of Codex lists (`visibility = "list"`), in the order
/// of `priority`. `None` when the text is not a catalog.
pub fn catalog(text: &str) -> Option<Vec<ModelChoice>> {
    let file: File = serde_json::from_str(text).ok()?;
    let mut listed: Vec<Entry> = file
        .models
        .into_iter()
        .filter(|entry| entry.visibility.as_deref() == Some("list") && !entry.slug.is_empty())
        .collect();
    listed.sort_by_key(|entry| entry.priority.unwrap_or(i64::MAX));
    Some(
        listed
            .into_iter()
            .map(|entry| ModelChoice {
                label: entry
                    .display_name
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| entry.slug.clone()),
                id: entry.slug,
                efforts: entry
                    .supported_reasoning_levels
                    .into_iter()
                    .map(|level| level.effort)
                    .collect(),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_listed_models_come_in_the_order_of_priority() {
        let text = r#"{
            "fetched_at": "2026-10-11T08:00:00Z",
            "models": [
                {"slug": "gpt-b", "display_name": "GPT-B", "visibility": "list", "priority": 5,
                 "supported_reasoning_levels": [{"effort": "low", "description": "Fast"},
                                                {"effort": "high", "description": "Deep"}]},
                {"slug": "gpt-hidden", "display_name": "Hidden", "visibility": "hide", "priority": 1},
                {"slug": "gpt-a", "visibility": "list", "priority": 2, "extra": true}
            ]
        }"#;
        let models = catalog(text).expect("a catalog");
        let names: Vec<_> = models
            .iter()
            .map(|m| (m.id.as_str(), m.label.as_str()))
            .collect();
        assert_eq!(names, [("gpt-a", "gpt-a"), ("gpt-b", "GPT-B")]);
        assert!(models[0].efforts.is_empty());
        assert_eq!(models[1].efforts, ["low", "high"]);
    }

    #[test]
    fn a_file_that_is_not_a_catalog_gives_none() {
        assert!(catalog("not json").is_none());
        assert!(catalog("[1, 2]").is_none());
        assert_eq!(catalog("{}").map(|models| models.len()), Some(0));
    }
}
