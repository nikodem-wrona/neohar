//! Pane tokens.
//!
//! A pane has a random token for its lifetime. A request with a valid token has pane scope, and
//! pane scope can call the `adapter.*` methods and nothing else. The token binds a call
//! to the process tree of one pane. It is not containment: any process of the same user can read
//! the environment of the pane.
//!
//! The registry maps a token to the ULID of its pane. The pane runtime issues a token when a pane
//! starts and revokes it when the pane is gone. A test or a recording registers a fixed token.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use ulid::Ulid;

use crate::error::RpcError;

/// The tokens of the panes of this core.
#[derive(Default)]
pub struct PaneTokens {
    by_token: Mutex<HashMap<String, Ulid>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl PaneTokens {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Makes a token of 32 random bytes (64 hex digits) for a pane. A pane that has one gets a
    /// new one, and the old one stops working.
    pub fn issue(&self, pane: Ulid) -> Result<String, RpcError> {
        let mut random = [0u8; 32];
        getrandom::fill(&mut random)
            .map_err(|e| RpcError::internal(format!("no random bytes: {e}")))?;
        let token: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let mut tokens = lock(&self.by_token);
        tokens.retain(|_, owner| *owner != pane);
        tokens.insert(token.clone(), pane);
        Ok(token)
    }

    /// Registers a token that the caller chose, for example in a test or a recording.
    pub fn register(&self, pane: Ulid, token: impl Into<String>) {
        lock(&self.by_token).insert(token.into(), pane);
    }

    /// The pane that a token belongs to.
    pub fn resolve(&self, token: &str) -> Option<Ulid> {
        lock(&self.by_token).get(token).copied()
    }

    /// Ends the tokens of a pane.
    pub fn revoke_pane(&self, pane: Ulid) {
        lock(&self.by_token).retain(|_, owner| *owner != pane);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(n: u128) -> Ulid {
        Ulid::from(n)
    }

    #[test]
    fn a_token_is_64_hex_digits_and_belongs_to_its_pane() {
        let tokens = PaneTokens::new();
        let token = tokens.issue(pane(1)).expect("a token");
        assert_eq!(token.len(), 64);
        assert!(token.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(tokens.resolve(&token), Some(pane(1)));
        assert_eq!(tokens.resolve("nope"), None);
        assert_eq!(tokens.resolve(""), None);
    }

    #[test]
    fn two_tokens_differ_and_a_new_one_replaces_the_old_one() {
        let tokens = PaneTokens::new();
        let first = tokens.issue(pane(1)).expect("a token");
        let other = tokens.issue(pane(2)).expect("a token");
        assert_ne!(first, other);
        let second = tokens.issue(pane(1)).expect("a token");
        assert_eq!(tokens.resolve(&first), None, "the old token stopped");
        assert_eq!(tokens.resolve(&second), Some(pane(1)));
        assert_eq!(tokens.resolve(&other), Some(pane(2)));
    }

    #[test]
    fn revoke_ends_the_tokens_of_one_pane() {
        let tokens = PaneTokens::new();
        let a = tokens.issue(pane(1)).expect("a token");
        let b = tokens.issue(pane(2)).expect("a token");
        tokens.register(pane(1), "fixed");
        tokens.revoke_pane(pane(1));
        assert_eq!(tokens.resolve(&a), None);
        assert_eq!(tokens.resolve("fixed"), None);
        assert_eq!(tokens.resolve(&b), Some(pane(2)));
    }
}
