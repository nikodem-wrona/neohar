//! The price table. The file is `crates/nh-agents/prices.toml`.

use serde::Deserialize;

use super::TokenUsage;

const TABLE: &str = include_str!("../../prices.toml");

/// Prices in US dollars for one million tokens.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Rates {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    /// The write to the cache of one hour. It is `cache_write` when the table has no figure.
    pub cache_write_1h: Option<f64>,
}

/// The second price of a model for a long prompt.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct Long {
    /// The prompt must have more tokens than this.
    pub over: u64,
    #[serde(flatten)]
    pub rates: Rates,
}

#[derive(Clone, Debug, Deserialize)]
struct Model {
    prefix: String,
    #[serde(flatten)]
    rates: Rates,
    long: Option<Long>,
}

#[derive(Debug, Deserialize)]
struct File {
    #[serde(default)]
    model: Vec<Model>,
}

/// The prices of the models.
#[derive(Clone, Debug)]
pub struct Prices {
    /// Sorted by the length of the prefix, the longest first.
    models: Vec<Model>,
}

impl Prices {
    /// The table that ships with neohar.
    pub fn bundled() -> Self {
        Self::parse(TABLE).expect("the bundled price table is valid")
    }

    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        let mut file: File = toml::from_str(text)?;
        file.model
            .sort_by_key(|model| std::cmp::Reverse(model.prefix.len()));
        Ok(Self { models: file.model })
    }

    /// The prices of a model: the entry with the longest prefix of the id.
    pub fn find(&self, model: &str) -> Option<(&str, Rates, Option<Long>)> {
        let id = normalize(model);
        self.models
            .iter()
            .find(|entry| id.starts_with(&entry.prefix))
            .map(|entry| (entry.prefix.as_str(), entry.rates, entry.long))
    }

    /// The cost of the tokens of one call, or `None` when the table does not know the model.
    pub fn cost(&self, model: &str, usage: &TokenUsage) -> Option<f64> {
        let (_, base, long) = self.find(model)?;
        let rates = match long {
            Some(long) if usage.prompt() > long.over => long.rates,
            _ => base,
        };
        let one_hour = usage.cache_write_1h.min(usage.cache_write);
        let five_minutes = usage.cache_write - one_hour;
        let dollars = usage.input as f64 * rates.input
            + usage.output as f64 * rates.output
            + usage.cache_read as f64 * rates.cache_read
            + five_minutes as f64 * rates.cache_write
            + one_hour as f64 * rates.cache_write_1h.unwrap_or(rates.cache_write);
        Some(dollars / 1_000_000.0)
    }
}

/// The id of a model as the table writes it: lower case, and without the name of a cloud in front
/// (`anthropic.claude-opus-4-5-…` on Bedrock).
fn normalize(model: &str) -> String {
    let lower = model.to_lowercase();
    match lower.find("claude-") {
        Some(at) => lower[at..].to_owned(),
        None => lower,
    }
}
