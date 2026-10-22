//! The models and the effort levels that a launch can choose: the menus of the form that
//! creates a task, and the check of an effort level in `agent.start`.

use nh_proto::api::{
    calls::agent::{HarnessChoices, ModelChoice},
    common::Harness,
    model::LoginState,
};

use super::schema::Launch;

/// The value of `model_catalog` that names the model catalog of Codex.
pub const CODEX_CATALOG: &str = "codex";

/// The longest effort level.
const EFFORT_MAX: usize = 32;

/// Whether an effort level is a word that a command line can carry: ASCII letters, digits, `-`
/// and `_`, at most 32 of them. The level goes into the argument that `effort_arg` gives, and that
/// argument can be TOML text, as for Codex.
pub fn valid_effort(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= EFFORT_MAX
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

/// The menus of a harness. `catalog` is the list that `model_catalog` names, when the core could
/// read it. An empty catalog counts as none, so the list of the manifest stays.
///
/// A launch without `model_arg` offers no model, and a launch without `effort_arg` offers no
/// effort level.
pub fn of(
    harness: Harness,
    launch: Option<&Launch>,
    catalog: Option<Vec<ModelChoice>>,
) -> HarnessChoices {
    let Some(launch) = launch else {
        return HarnessChoices {
            harness,
            models: Vec::new(),
            efforts: Vec::new(),
            ..not_checked()
        };
    };
    let efforts: Vec<String> = if launch.effort_arg.is_some() {
        launch
            .efforts
            .iter()
            .filter(|level| valid_effort(level))
            .cloned()
            .collect()
    } else {
        Vec::new()
    };
    let mut models = if launch.model_arg.is_some() {
        catalog
            .filter(|models| !models.is_empty())
            .unwrap_or_else(|| {
                launch
                    .models
                    .iter()
                    .map(|entry| ModelChoice {
                        id: entry.id.clone(),
                        label: entry.label.clone().unwrap_or_else(|| entry.id.clone()),
                        efforts: Vec::new(),
                    })
                    .collect()
            })
    } else {
        Vec::new()
    };
    for model in &mut models {
        if launch.effort_arg.is_none() {
            model.efforts.clear();
        } else {
            model.efforts.retain(|level| valid_effort(level));
        }
    }
    HarnessChoices {
        harness,
        models,
        efforts,
        ..not_checked()
    }
}

/// The state of a harness before the core fills it from the projection `harnesses`.
fn not_checked() -> HarnessChoices {
    HarnessChoices {
        harness: Harness::Claude,
        models: Vec::new(),
        efforts: Vec::new(),
        installed: false,
        login: LoginState::Unknown,
        available: false,
        hint: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::schema::ModelEntry;

    fn launch() -> Launch {
        Launch {
            argv: vec!["claude".to_owned()],
            model_arg: Some(vec!["--model".to_owned(), "{model}".to_owned()]),
            effort_arg: Some(vec!["--effort".to_owned(), "{effort}".to_owned()]),
            models: vec![
                ModelEntry {
                    id: "opus".to_owned(),
                    label: Some("Opus".to_owned()),
                },
                ModelEntry {
                    id: "haiku".to_owned(),
                    label: None,
                },
            ],
            efforts: vec!["low".to_owned(), "high".to_owned()],
            ..Launch::default()
        }
    }

    fn catalog() -> Vec<ModelChoice> {
        vec![ModelChoice {
            id: "gpt-5.5".to_owned(),
            label: "GPT-5.5".to_owned(),
            efforts: vec!["low".to_owned(), "x\"y".to_owned()],
        }]
    }

    #[test]
    fn an_effort_level_is_one_plain_word() {
        assert!(valid_effort("xhigh"));
        assert!(valid_effort("very_high-2"));
        for bad in ["", "a b", "high\"", "$(x)", &"a".repeat(33)] {
            assert!(!valid_effort(bad), "{bad:?}");
        }
    }

    #[test]
    fn the_manifest_gives_the_menus_and_a_label_defaults_to_the_id() {
        let menus = of(Harness::Claude, Some(&launch()), None);
        let models: Vec<_> = menus
            .models
            .iter()
            .map(|m| (m.id.as_str(), m.label.as_str()))
            .collect();
        assert_eq!(models, [("opus", "Opus"), ("haiku", "haiku")]);
        assert_eq!(menus.efforts, ["low", "high"]);
    }

    #[test]
    fn a_catalog_comes_before_the_list_and_an_empty_one_does_not() {
        let menus = of(Harness::Codex, Some(&launch()), Some(catalog()));
        assert_eq!(menus.models.len(), 1);
        assert_eq!(menus.models[0].id, "gpt-5.5");
        // A level that a command line cannot carry is left out.
        assert_eq!(menus.models[0].efforts, ["low"]);
        let menus = of(Harness::Codex, Some(&launch()), Some(Vec::new()));
        assert_eq!(menus.models.len(), 2);
    }

    #[test]
    fn a_launch_without_the_arguments_offers_nothing() {
        let bare = Launch {
            model_arg: None,
            effort_arg: None,
            ..launch()
        };
        let menus = of(Harness::Claude, Some(&bare), Some(catalog()));
        assert!(menus.models.is_empty());
        assert!(menus.efforts.is_empty());
        let menus = of(
            Harness::Claude,
            Some(&Launch {
                effort_arg: None,
                ..launch()
            }),
            Some(catalog()),
        );
        assert!(menus.efforts.is_empty());
        assert!(menus.models[0].efforts.is_empty());
        assert!(of(Harness::Codex, None, None).models.is_empty());
    }
}
