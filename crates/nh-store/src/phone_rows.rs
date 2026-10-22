//! The paired phones: `phone_devices`. A row has the hash of the token of the
//! phone, never the token, and the push subscription of the phone when it has one.

use rusqlite::{Row, params};

use crate::{Store, StoreError, Tx};

/// A paired phone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhoneRow {
    /// The device id, a ULID.
    pub device: String,
    pub name: String,
    /// The BLAKE3 hash of the token, as hex.
    pub token_hash: String,
    pub paired_at: i64,
    /// The last connection of the phone.
    pub seen_at: Option<i64>,
    /// The push subscription of the phone.
    pub push: Option<PushRow>,
}

/// The push subscription of a phone: the endpoint of the push service and the keys of
/// the browser, as the phone sent them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushRow {
    pub endpoint: String,
    /// The public key of the browser (P-256), base64url.
    pub p256dh: String,
    /// The authentication secret of the browser, base64url.
    pub auth: String,
}

fn from_row(row: &Row<'_>) -> Result<PhoneRow, StoreError> {
    let endpoint: Option<String> = row.get(5)?;
    let p256dh: Option<String> = row.get(6)?;
    let auth: Option<String> = row.get(7)?;
    let push = match (endpoint, p256dh, auth) {
        (Some(endpoint), Some(p256dh), Some(auth)) => Some(PushRow {
            endpoint,
            p256dh,
            auth,
        }),
        _ => None,
    };
    Ok(PhoneRow {
        device: row.get(0)?,
        name: row.get(1)?,
        token_hash: row.get(2)?,
        paired_at: row.get(3)?,
        seen_at: row.get(4)?,
        push,
    })
}

/// The phones, the one paired last first.
fn read_phones(conn: &rusqlite::Connection) -> Result<Vec<PhoneRow>, StoreError> {
    let mut statement = conn.prepare(
        "SELECT device, name, token_hash, paired_at, seen_at, push_endpoint, push_p256dh, push_auth
         FROM phone_devices ORDER BY paired_at DESC, device",
    )?;
    let mut rows = statement.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(from_row(row)?);
    }
    Ok(out)
}

impl Tx<'_> {
    /// Adds a paired phone.
    pub fn put_phone(&self, row: &PhoneRow) -> Result<(), StoreError> {
        self.writable()?;
        let push = row.push.as_ref();
        self.sql().execute(
            "INSERT INTO phone_devices
             (device, name, token_hash, paired_at, seen_at, push_endpoint, push_p256dh, push_auth)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                row.device,
                row.name,
                row.token_hash,
                row.paired_at,
                row.seen_at,
                push.map(|p| &p.endpoint),
                push.map(|p| &p.p256dh),
                push.map(|p| &p.auth),
            ],
        )?;
        Ok(())
    }

    /// Sets or removes the push subscription of a phone. Says whether the phone exists.
    pub fn set_phone_push(&self, device: &str, push: Option<&PushRow>) -> Result<bool, StoreError> {
        self.writable()?;
        let changed = self.sql().execute(
            "UPDATE phone_devices SET push_endpoint = ?2, push_p256dh = ?3, push_auth = ?4
             WHERE device = ?1",
            params![
                device,
                push.map(|p| &p.endpoint),
                push.map(|p| &p.p256dh),
                push.map(|p| &p.auth),
            ],
        )?;
        Ok(changed > 0)
    }

    /// Changes the name of a phone. Says whether the phone exists.
    pub fn rename_phone(&self, device: &str, name: &str) -> Result<bool, StoreError> {
        self.writable()?;
        let changed = self.sql().execute(
            "UPDATE phone_devices SET name = ?2 WHERE device = ?1",
            params![device, name],
        )?;
        Ok(changed > 0)
    }

    /// Notes a connection of a phone. Says whether the phone exists.
    pub fn see_phone(&self, device: &str, at: i64) -> Result<bool, StoreError> {
        self.writable()?;
        let changed = self.sql().execute(
            "UPDATE phone_devices SET seen_at = ?2 WHERE device = ?1",
            params![device, at],
        )?;
        Ok(changed > 0)
    }

    /// Removes a phone. Says whether there was one.
    pub fn delete_phone(&self, device: &str) -> Result<bool, StoreError> {
        self.writable()?;
        let removed = self
            .sql()
            .execute("DELETE FROM phone_devices WHERE device = ?1", [device])?;
        Ok(removed > 0)
    }

    pub fn phones(&self) -> Result<Vec<PhoneRow>, StoreError> {
        read_phones(self.sql())
    }
}

impl Store {
    pub fn phones(&self) -> Result<Vec<PhoneRow>, StoreError> {
        read_phones(&self.conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(device: &str, hash: &str, at: i64) -> PhoneRow {
        PhoneRow {
            device: device.to_owned(),
            name: format!("phone {device}"),
            token_hash: hash.to_owned(),
            paired_at: at,
            seen_at: None,
            push: None,
        }
    }

    #[test]
    fn a_phone_is_kept_renamed_seen_and_removed() {
        let mut store = Store::open_in_memory().expect("a store");
        store
            .tx(1, |tx| {
                tx.put_phone(&row("a", "h1", 1))?;
                tx.put_phone(&row("b", "h2", 2))
            })
            .expect("two phones");
        let devices: Vec<String> = store
            .phones()
            .expect("the phones")
            .into_iter()
            .map(|row| row.device)
            .collect();
        assert_eq!(devices, ["b", "a"], "the phone paired last comes first");

        let (renamed, _) = store
            .tx(3, |tx| tx.rename_phone("a", "iPhone"))
            .expect("a rename");
        assert!(renamed);
        let (seen, _) = store.tx(4, |tx| tx.see_phone("a", 4)).expect("a visit");
        assert!(seen);
        let a = store
            .phones()
            .expect("the phones")
            .into_iter()
            .find(|row| row.device == "a")
            .expect("phone a");
        assert_eq!((a.name.as_str(), a.seen_at), ("iPhone", Some(4)));

        let (removed, _) = store.tx(5, |tx| tx.delete_phone("a")).expect("a removal");
        assert!(removed);
        let (again, _) = store.tx(6, |tx| tx.delete_phone("a")).expect("a removal");
        assert!(!again);
        assert_eq!(store.phones().expect("the phones").len(), 1);
    }

    #[test]
    fn a_push_subscription_is_kept_and_removed() {
        let mut store = Store::open_in_memory().expect("a store");
        store
            .tx(1, |tx| tx.put_phone(&row("a", "h1", 1)))
            .expect("a phone");
        let push = PushRow {
            endpoint: "https://web.push.apple.com/abc".to_owned(),
            p256dh: "BCVx".to_owned(),
            auth: "BTBZ".to_owned(),
        };
        let (set, _) = store
            .tx(2, |tx| tx.set_phone_push("a", Some(&push)))
            .expect("a subscription");
        assert!(set);
        let phones = store.phones().expect("the phones");
        assert_eq!(phones[0].push.as_ref(), Some(&push));

        let (cleared, _) = store
            .tx(3, |tx| tx.set_phone_push("a", None))
            .expect("no subscription");
        assert!(cleared);
        assert_eq!(store.phones().expect("the phones")[0].push, None);
        let (missing, _) = store
            .tx(4, |tx| tx.set_phone_push("b", Some(&push)))
            .expect("no phone");
        assert!(!missing);
    }

    #[test]
    fn two_phones_cannot_share_a_token() {
        let mut store = Store::open_in_memory().expect("a store");
        store
            .tx(1, |tx| tx.put_phone(&row("a", "same", 1)))
            .expect("a phone");
        assert!(
            store
                .tx(2, |tx| tx.put_phone(&row("b", "same", 2)))
                .is_err()
        );
    }
}
