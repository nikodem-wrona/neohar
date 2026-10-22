//! The adapter that reads the `[states]` table of a manifest.
//!
//! Both harnesses send their hooks with the same fields: `session_id`, `transcript_path`, `model`,
//! and `source` and `reason` where the event has them. A manifest says which events mean
//! `working`, `idle`, `error` or the end of the session. This adapter reads both, so a run shows
//! its execution state from its first hook. It opens no interaction and never decides: an event
//! that opens one (`open:approval`) goes on to the gate of the core as before. The adapters of
//! Claude Code and Codex read the full payloads.

use std::{collections::BTreeMap, path::PathBuf};

use async_trait::async_trait;
use serde_json::Value;

use super::{
    Adapter, AdapterCaps, AdapterCtx, AdapterSignal, Result, SignalReply, StartSource, Transport,
};
use crate::manifest::Manifest;

/// The key of an event in the `[states]` table: `hook:Stop`.
fn state_key(event: &str) -> String {
    format!("hook:{event}")
}

pub struct ManifestAdapter {
    ctx: Option<AdapterCtx>,
    states: BTreeMap<String, String>,
    /// The session id the adapter reported last.
    session: Option<String>,
}

impl ManifestAdapter {
    pub fn new(manifest: &Manifest) -> Self {
        Self {
            ctx: None,
            states: manifest.states.clone(),
            session: None,
        }
    }

    fn emit(&self, signal: AdapterSignal) {
        if let Some(ctx) = &self.ctx {
            ctx.emitter.emit(signal);
        }
    }
}

fn text<'a>(payload: &'a Value, key: &str) -> Option<&'a str> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
}

#[async_trait]
impl Adapter for ManifestAdapter {
    fn capabilities(&self) -> AdapterCaps {
        AdapterCaps {
            transports: vec![Transport::Hooks],
            observe: true,
            ..AdapterCaps::default()
        }
    }

    async fn start(&mut self, ctx: AdapterCtx) -> Result<()> {
        self.ctx = Some(ctx);
        Ok(())
    }

    async fn on_signal(&mut self, signal: AdapterSignal) -> Result<SignalReply> {
        let AdapterSignal::Raw {
            harness_event,
            payload,
        } = signal
        else {
            return Ok(SignalReply::Continue);
        };
        let event = harness_event.as_str();
        // Every hook names its session. A new name is a new session of the same process
        // (`/clear`, `/resume`, a fork), and the first hook of a run is the first name.
        if let Some(id) = text(&payload, "session_id")
            && self.session.as_deref() != Some(id)
        {
            self.session = Some(id.to_owned());
            self.emit(AdapterSignal::SessionStarted {
                session_id: id.to_owned(),
                transcript_path: text(&payload, "transcript_path").map(PathBuf::from),
                // A hook that is not `SessionStart` names a session that this adapter did not see
                // begin (the core started in the middle of it), and says nothing about the state.
                source: text(&payload, "source")
                    .map_or_else(|| StartSource::Other("seen".to_owned()), StartSource::parse),
                model: text(&payload, "model").map(str::to_owned),
            });
        }
        match self.states.get(&state_key(event)).map(String::as_str) {
            Some("working") => self.emit(AdapterSignal::TurnStarted {
                turn_ref: text(&payload, "turn_id")
                    .or_else(|| text(&payload, "prompt_id"))
                    .map(str::to_owned),
                prompt_preview: None,
            }),
            Some("idle") => self.emit(AdapterSignal::TurnEnded {
                turn_ref: text(&payload, "turn_id").map(str::to_owned),
                stop_reason: None,
                last_message_preview: None,
            }),
            Some("error") => self.emit(AdapterSignal::Error {
                kind: event.to_owned(),
                message: text(&payload, "error")
                    .or_else(|| text(&payload, "message"))
                    .unwrap_or("the harness reported an error")
                    .to_owned(),
                retrying: false,
            }),
            Some("session_ended") => self.emit(AdapterSignal::SessionEnded {
                reason: text(&payload, "reason").unwrap_or("ended").to_owned(),
            }),
            // `open:<kind>` and events that the table does not know add nothing here.
            _ => {}
        }
        self.emit(AdapterSignal::Raw {
            harness_event,
            payload,
        });
        Ok(SignalReply::Continue)
    }
}
