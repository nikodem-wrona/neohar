//! The degraded mode: the disk is full, or the database has an I/O error.
//!
//! The core then stops accepting mutations that need persistence. It keeps the panes and the
//! holders running, shows a banner, and says which commands are off. It never pretends to persist.
//! This module is the part of the store: it recognizes the errors, remembers that it is degraded,
//! refuses writes while it is, and takes the probe that leaves the mode.
//!
//! The errors are `SQLITE_FULL`, `SQLITE_IOERR*`, `SQLITE_READONLY` and `SQLITE_CANTOPEN`. All of
//! them become [`StoreError::Degraded`], whichever function of the store got them.

use crate::{Store, StoreError};

/// How often the core tries a write to leave the degraded mode.
pub const PROBE_INTERVAL_MS: i64 = 10_000;

/// A fault for tests (`fault-injection`). The write path of the store acts as if SQLite returned it.
#[cfg(any(test, feature = "fault-injection"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// `SQLITE_FULL`
    DiskFull,
    /// `SQLITE_IOERR`
    IoError,
    /// `SQLITE_READONLY`
    ReadOnly,
    /// `SQLITE_CANTOPEN`
    CannotOpen,
}

#[cfg(any(test, feature = "fault-injection"))]
impl Fault {
    fn error(self) -> rusqlite::Error {
        use rusqlite::ffi;
        let code = match self {
            Self::DiskFull => ffi::SQLITE_FULL,
            Self::IoError => ffi::SQLITE_IOERR,
            Self::ReadOnly => ffi::SQLITE_READONLY,
            Self::CannotOpen => ffi::SQLITE_CANTOPEN,
        };
        rusqlite::Error::SqliteFailure(ffi::Error::new(code), Some("injected".to_owned()))
    }
}

/// What the store knows about its own health.
#[derive(Debug, Default)]
pub(crate) struct Health {
    /// The reason of the first error, while the store is degraded.
    degraded: Option<String>,
    last_probe_ms: Option<i64>,
    #[cfg(any(test, feature = "fault-injection"))]
    fault: Option<Fault>,
}

/// The text of the reason, or `None` when the error is not one of the four.
pub(crate) fn classify(error: &rusqlite::Error) -> Option<String> {
    use rusqlite::ErrorCode;
    let rusqlite::Error::SqliteFailure(failure, message) = error else {
        return None;
    };
    let what = match failure.code {
        ErrorCode::DiskFull => "the disk is full",
        ErrorCode::SystemIoFailure => "the disk reports an I/O error",
        ErrorCode::ReadOnly => "the database is read-only",
        ErrorCode::CannotOpen => "the database file cannot be opened",
        _ => return None,
    };
    Some(match message {
        Some(message) => format!("{what} ({message})"),
        None => what.to_owned(),
    })
}

/// The reason for an I/O error of a file that the store writes (a blob, for example), or `None`.
pub(crate) fn classify_io(error: &std::io::Error) -> Option<String> {
    use std::io::ErrorKind;
    // ENOSPC, EROFS and EIO. The kinds `StorageFull` and `ReadOnlyFilesystem` cover the first two.
    const EIO: i32 = 5;
    match (error.kind(), error.raw_os_error()) {
        (ErrorKind::StorageFull, _) => Some("the disk is full".to_owned()),
        (ErrorKind::ReadOnlyFilesystem, _) => Some("the file system is read-only".to_owned()),
        (_, Some(EIO)) => Some("the disk reports an I/O error".to_owned()),
        _ => None,
    }
}

impl Store {
    /// Whether the store refuses writes because of a disk error.
    pub fn is_degraded(&self) -> bool {
        self.health.borrow().degraded.is_some()
    }

    /// Why the store is degraded.
    pub fn degraded_reason(&self) -> Option<String> {
        self.health.borrow().degraded.clone()
    }

    /// Refuses a write while the store is degraded, and applies an injected fault.
    pub(crate) fn guard(&self) -> Result<(), StoreError> {
        if let Some(reason) = &self.health.borrow().degraded {
            return Err(StoreError::Degraded {
                reason: reason.clone(),
            });
        }
        self.check_fault()
    }

    #[cfg(any(test, feature = "fault-injection"))]
    fn check_fault(&self) -> Result<(), StoreError> {
        let fault = self.health.borrow().fault;
        match fault {
            Some(fault) => self.note(Err(StoreError::from(fault.error()))),
            None => Ok(()),
        }
    }

    #[cfg(not(any(test, feature = "fault-injection")))]
    fn check_fault(&self) -> Result<(), StoreError> {
        Ok(())
    }

    /// Notes the result of a write. A degraded error puts the store in the degraded mode.
    pub(crate) fn note<T>(&self, result: Result<T, StoreError>) -> Result<T, StoreError> {
        if let Err(StoreError::Degraded { reason }) = &result {
            let mut health = self.health.borrow_mut();
            if health.degraded.is_none() {
                health.degraded = Some(reason.clone());
            }
        }
        result
    }

    /// Runs one statement that writes.
    pub(crate) fn exec<P: rusqlite::Params>(
        &self,
        sql: &str,
        params: P,
    ) -> Result<usize, StoreError> {
        self.guard()?;
        self.note(self.conn.execute(sql, params).map_err(StoreError::from))
    }

    /// Tries a small write. On success the store leaves the degraded mode. Returns whether the
    /// store accepts writes now.
    pub fn probe(&mut self, now_ms: i64) -> Result<bool, StoreError> {
        self.health.borrow_mut().last_probe_ms = Some(now_ms);
        let write = self.check_fault().and_then(|()| {
            self.conn
                .execute(
                    "INSERT INTO meta (key, value) VALUES ('last_probe', ?1)
                         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                    [now_ms.to_string()],
                )
                .map_err(StoreError::from)
        });
        match write {
            Ok(_) => {
                self.health.borrow_mut().degraded = None;
                Ok(true)
            }
            Err(StoreError::Degraded { .. }) => Ok(false),
            Err(other) => Err(other),
        }
    }

    /// Probes when the store is degraded and the last probe is [`PROBE_INTERVAL_MS`] old. Returns
    /// `Some(accepts_writes)` when it probed.
    pub fn maybe_probe(&mut self, now_ms: i64) -> Result<Option<bool>, StoreError> {
        let due = {
            let health = self.health.borrow();
            health.degraded.is_some()
                && health
                    .last_probe_ms
                    .is_none_or(|last| now_ms.saturating_sub(last) >= PROBE_INTERVAL_MS)
        };
        if due {
            self.probe(now_ms).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Makes the write path fail with `fault`, or work again with `None` (for tests).
    #[cfg(any(test, feature = "fault-injection"))]
    pub fn inject_fault(&mut self, fault: Option<Fault>) {
        self.health.borrow_mut().fault = fault;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        Store::open_in_memory().expect("a store")
    }

    #[test]
    fn the_four_sqlite_errors_are_degraded_and_others_are_not() {
        use rusqlite::{ErrorCode, ffi};
        let failure = |code: i32| rusqlite::Error::SqliteFailure(ffi::Error::new(code), None);
        for code in [
            ffi::SQLITE_FULL,
            ffi::SQLITE_IOERR,
            ffi::SQLITE_IOERR_WRITE,
            ffi::SQLITE_IOERR_FSYNC,
            ffi::SQLITE_READONLY,
            ffi::SQLITE_READONLY_DBMOVED,
            ffi::SQLITE_CANTOPEN,
        ] {
            assert!(
                matches!(StoreError::from(failure(code)), StoreError::Degraded { .. }),
                "code {code}"
            );
        }
        for code in [
            ffi::SQLITE_CONSTRAINT,
            ffi::SQLITE_BUSY,
            ffi::SQLITE_CORRUPT,
            ffi::SQLITE_ERROR,
        ] {
            assert!(
                matches!(StoreError::from(failure(code)), StoreError::Sqlite(_)),
                "code {code}"
            );
        }
        assert_eq!(ffi::Error::new(ffi::SQLITE_FULL).code, ErrorCode::DiskFull);
        assert!(classify(&rusqlite::Error::QueryReturnedNoRows).is_none());
    }

    #[test]
    fn an_io_error_of_a_full_or_read_only_disk_is_degraded() {
        use std::io::{Error, ErrorKind};
        assert!(classify_io(&Error::from(ErrorKind::StorageFull)).is_some());
        assert!(classify_io(&Error::from(ErrorKind::ReadOnlyFilesystem)).is_some());
        assert!(classify_io(&Error::from_raw_os_error(5)).is_some());
        assert!(classify_io(&Error::from(ErrorKind::NotFound)).is_none());
    }

    #[test]
    fn a_fault_puts_the_store_in_the_degraded_mode_and_writes_stop() {
        let mut store = store();
        assert!(!store.is_degraded());
        store.set_meta("a", "1").expect("a write");

        store.inject_fault(Some(Fault::DiskFull));
        let error = store.set_meta("b", "2").expect_err("the write fails");
        assert!(matches!(&error, StoreError::Degraded { reason } if reason.contains("full")));
        assert!(store.is_degraded());
        assert!(store.degraded_reason().expect("a reason").contains("full"));

        // Even with the fault gone, the store refuses writes until a probe works.
        store.inject_fault(None);
        assert!(matches!(
            store.set_meta("c", "3"),
            Err(StoreError::Degraded { .. })
        ));
        assert!(matches!(
            store.tx(1, |_| Ok(())),
            Err(StoreError::Degraded { .. })
        ));
        // Reads work. The store never lies about what it holds.
        assert_eq!(store.meta("a").expect("a read").as_deref(), Some("1"));
        assert_eq!(store.meta("b").expect("a read"), None);
    }

    #[test]
    fn the_probe_fails_while_the_fault_lasts_and_ends_the_mode_when_it_is_gone() {
        let mut store = store();
        store.inject_fault(Some(Fault::IoError));
        assert!(store.set_meta("x", "1").is_err());
        assert!(
            !store.probe(1_000).expect("a probe"),
            "the fault is still there"
        );
        assert!(store.is_degraded());

        store.inject_fault(None);
        assert!(store.probe(2_000).expect("a probe"));
        assert!(!store.is_degraded());
        store.set_meta("x", "2").expect("writes work again");
        assert_eq!(store.meta("x").expect("a read").as_deref(), Some("2"));
    }

    #[test]
    fn the_store_probes_every_ten_seconds_and_only_while_degraded() {
        let mut store = store();
        assert_eq!(
            store.maybe_probe(0).expect("a call"),
            None,
            "healthy: no probe"
        );

        store.inject_fault(Some(Fault::ReadOnly));
        assert!(store.set_meta("x", "1").is_err());
        assert_eq!(
            store.maybe_probe(100_000).expect("a call"),
            Some(false),
            "the first probe"
        );
        assert_eq!(
            store.maybe_probe(105_000).expect("a call"),
            None,
            "too early"
        );
        assert_eq!(
            store.maybe_probe(109_999).expect("a call"),
            None,
            "too early"
        );
        store.inject_fault(None);
        assert_eq!(
            store.maybe_probe(110_000).expect("a call"),
            Some(true),
            "ten seconds later"
        );
        assert!(!store.is_degraded());
        assert_eq!(
            store.maybe_probe(130_000).expect("a call"),
            None,
            "healthy again"
        );
    }

    #[test]
    fn a_fault_inside_a_transaction_rolls_it_back_and_degrades() {
        let mut store = store();
        store.inject_fault(Some(Fault::CannotOpen));
        let result = store.tx(1, |tx| {
            tx.append_raw("pane.created", 1, None, serde_json::json!({}))
        });
        assert!(matches!(result, Err(StoreError::Degraded { .. })));
        store.inject_fault(None);
        assert!(store.probe(5).expect("a probe"));
        assert_eq!(
            store.last_seq().expect("a seq"),
            0,
            "no event of the failed transaction"
        );
    }

    #[test]
    fn a_degraded_error_from_a_statement_inside_the_transaction_degrades_too() {
        let mut store = store();
        // The error comes from SQLite inside the closure, not from the guard.
        let result: Result<((), Vec<_>), StoreError> = store.tx(1, |_| {
            Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_FULL),
                None,
            )
            .into())
        });
        assert!(matches!(result, Err(StoreError::Degraded { .. })));
        assert!(store.is_degraded());
    }

    #[test]
    fn a_real_full_database_is_degraded_without_any_injection() {
        let mut store = store();
        // SQLite refuses to grow the file past `max_page_count`, and answers `SQLITE_FULL`.
        let pages: i64 = store
            .conn
            .pragma_query_value(None, "page_count", |row| row.get(0))
            .expect("a page count");
        let _: i64 = store
            .conn
            .pragma_update_and_check(None, "max_page_count", pages, |row| row.get(0))
            .expect("a limit");
        let error = store
            .set_meta("big", &"x".repeat(2_000_000))
            .expect_err("the database cannot grow");
        assert!(
            matches!(&error, StoreError::Degraded { reason } if reason.contains("full")),
            "{error}"
        );
        assert!(store.is_degraded());
        // The failed write left nothing behind, and a small read still works.
        assert_eq!(store.meta("big").expect("a read"), None);
        store.integrity_check().expect("the database is sound");
        // With room again, the probe ends the mode.
        let _: i64 = store
            .conn
            .pragma_update_and_check(None, "max_page_count", 1_000_000, |row| row.get(0))
            .expect("a limit");
        assert!(store.probe(1).expect("a probe"));
        store.set_meta("small", "1").expect("a write");
    }

    #[test]
    fn a_read_transaction_works_while_the_store_is_degraded_and_cannot_write() {
        let mut store = store();
        store.set_meta("k", "v").expect("a write");
        store.inject_fault(Some(Fault::DiskFull));
        assert!(store.set_meta("x", "1").is_err());
        assert!(store.is_degraded());
        let value = store
            .tx_read(1, |tx| tx.meta("k"))
            .expect("a read works in the degraded mode");
        assert_eq!(value.as_deref(), Some("v"));
        let error = store
            .tx_read(2, |tx| {
                tx.append_raw("pane.created", 1, None, serde_json::json!({}))
            })
            .expect_err("a read transaction cannot append");
        assert!(matches!(error, StoreError::BadValue(m) if m.contains("read transaction")));
        assert!(store.tx_read(3, |tx| tx.next_handle("task")).is_err());
        assert!(store.tx_read(4, |tx| tx.bump_projection("tree")).is_err());
    }

    #[test]
    fn the_first_reason_stays() {
        let mut store = store();
        store.inject_fault(Some(Fault::DiskFull));
        assert!(store.set_meta("a", "1").is_err());
        store.inject_fault(Some(Fault::IoError));
        assert!(store.set_meta("b", "2").is_err());
        assert!(store.degraded_reason().expect("a reason").contains("full"));
    }
}
