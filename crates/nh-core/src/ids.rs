//! New ids.

use nh_proto::api::ids::{InteractionId, NotificationId};
use ulid::Ulid;

use crate::{error::RpcError, persist::now_ms};

/// A new ULID. The ids sort by the time of their creation.
pub(crate) fn new_ulid() -> Result<Ulid, RpcError> {
    let mut random = [0u8; 16];
    getrandom::fill(&mut random)
        .map_err(|e| RpcError::internal(format!("no random bytes: {e}")))?;
    let ms = u64::try_from(now_ms()).unwrap_or(0);
    let tail = u128::from_be_bytes(random) & ((1u128 << 80) - 1);
    Ok(Ulid::from_parts(ms, tail))
}

/// A new id for an entry of the `notifications` projection.
pub(crate) fn new_notification_id() -> Result<NotificationId, RpcError> {
    new_ulid().map(NotificationId::new)
}

/// A new id for an interaction.
pub(crate) fn new_interaction_id() -> Result<InteractionId, RpcError> {
    new_ulid().map(InteractionId::new)
}

/// A new random UUID (version 4) in its text form, for the session id that the core gives a
/// harness.
pub(crate) fn new_uuid() -> Result<String, RpcError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| RpcError::internal(format!("no random bytes: {e}")))?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_uuid_has_the_shape_of_version_four() {
        let id = new_uuid().expect("a uuid");
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(
            parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
            [8, 4, 4, 4, 12]
        );
        assert!(parts[2].starts_with('4'));
        assert!(matches!(parts[3].as_bytes()[0], b'8' | b'9' | b'a' | b'b'));
        assert_ne!(id, new_uuid().expect("another"));
    }
}
