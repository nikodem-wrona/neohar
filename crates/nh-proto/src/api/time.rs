//! Time in the API: RFC 3339 in UTC with milliseconds, for example
//! `2026-10-09T12:34:56.789Z`. The store keeps the same moment as unix milliseconds.

use std::{borrow::Cow, fmt};

use chrono::{DateTime, SecondsFormat, Utc};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// A moment, in unix milliseconds. On the wire it is a text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Timestamp(pub i64);

impl Timestamp {
    /// The RFC 3339 text of the moment. A moment that no calendar date holds gives the epoch.
    pub fn to_rfc3339(self) -> String {
        DateTime::<Utc>::from_timestamp_millis(self.0)
            .unwrap_or_default()
            .to_rfc3339_opts(SecondsFormat::Millis, true)
    }

    /// Parses RFC 3339 with any offset. The result is in UTC, in whole milliseconds.
    pub fn parse(text: &str) -> Result<Self, chrono::ParseError> {
        DateTime::parse_from_rfc3339(text).map(|moment| Self(moment.timestamp_millis()))
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_rfc3339())
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_rfc3339())
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(|e| de::Error::custom(format!("not an RFC 3339 time: {e}")))
    }
}

impl JsonSchema for Timestamp {
    fn schema_name() -> Cow<'static, str> {
        "Timestamp".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        schemars::json_schema!({
            "type": "string",
            "format": "date-time",
            "description": "RFC 3339 in UTC with milliseconds, for example 2026-10-09T12:34:56.789Z."
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_moment_is_utc_text_with_milliseconds() {
        let moment = Timestamp(1_791_549_296_789);
        assert_eq!(moment.to_rfc3339(), "2026-10-09T12:34:56.789Z");
        let json = serde_json::to_string(&moment).expect("serializes");
        assert_eq!(json, "\"2026-10-09T12:34:56.789Z\"");
        assert_eq!(
            serde_json::from_str::<Timestamp>(&json).expect("decodes"),
            moment
        );
    }

    #[test]
    fn the_epoch_and_a_moment_before_it_work() {
        assert_eq!(Timestamp(0).to_rfc3339(), "1970-01-01T00:00:00.000Z");
        let before = Timestamp(-1);
        assert_eq!(
            Timestamp::parse(&before.to_rfc3339()).expect("parses"),
            before
        );
    }

    #[test]
    fn an_offset_is_converted_to_utc() {
        let a = Timestamp::parse("2026-10-09T14:34:56.789+02:00").expect("parses");
        assert_eq!(a, Timestamp(1_791_549_296_789));
        assert!(Timestamp::parse("2026-10-09").is_err());
        assert!(Timestamp::parse("tomorrow").is_err());
    }
}
