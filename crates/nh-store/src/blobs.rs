//! The blob store: content-addressed files for screenshots, large tool inputs and card
//! images. A blob is `<state dir>/blobs/<hash[0..2]>/<hash>`, where the hash is the BLAKE3 digest in
//! lower-case hex. The `blobs` table has the size, the type and a count of the rows that name the
//! blob. Only the files of tasks count their references, so the store deletes no blob.
//!
//! A file is written to a temporary name in its directory, synced, and renamed. A crash leaves
//! either no file or the whole file. The directories have mode 0700 and the files 0600.

use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    io::{self, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use rusqlite::{OptionalExtension, params};

use crate::{Store, StoreError, Tx, degraded};

/// What the store knows about a blob.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobInfo {
    pub hash: String,
    pub bytes: u64,
    pub mime: String,
    pub created_at_ms: i64,
    /// How many rows name the blob.
    pub refs: u32,
}

/// A number for the names of temporary files, so that two writers never share one.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A hash is 64 characters of lower-case hex. Anything else is not a blob name, so a caller
/// cannot reach another file with `..` or a path.
fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Store {
    /// The directory of the database. A store in memory has none.
    pub(crate) fn data_dir(&self) -> Result<&Path, StoreError> {
        self.dir
            .as_deref()
            .ok_or_else(|| StoreError::BadValue("this store has no directory".to_owned()))
    }

    fn blob_root(&self) -> Result<PathBuf, StoreError> {
        Ok(self.data_dir()?.join("blobs"))
    }

    /// The path of the file of a blob. The file may not exist.
    pub fn blob_path(&self, hash: &str) -> Result<PathBuf, StoreError> {
        if !valid_hash(hash) {
            return Err(StoreError::BadValue(format!("not a blob hash: {hash:?}")));
        }
        Ok(self.blob_root()?.join(&hash[..2]).join(hash))
    }

    /// An error of a file that the store writes. A full or read-only disk puts the store in the
    /// degraded mode, like the same error of the database.
    fn blob_io<T>(&self, path: &Path, result: io::Result<T>) -> Result<T, StoreError> {
        self.note(
            result.map_err(|source| match degraded::classify_io(&source) {
                Some(reason) => StoreError::Degraded { reason },
                None => StoreError::Io {
                    path: path.to_owned(),
                    source,
                },
            }),
        )
    }

    /// Stores bytes and returns their hash. The same bytes always give the same hash, and a second
    /// call writes nothing.
    pub fn put_blob(&self, bytes: &[u8], mime: &str, now_ms: i64) -> Result<String, StoreError> {
        self.guard()?;
        let hash = blake3::hash(bytes).to_hex().to_string();
        let path = self.blob_path(&hash)?;
        let dir = path.parent().unwrap_or(Path::new("."));
        let exists = fs::metadata(&path).is_ok_and(|m| m.len() == bytes.len() as u64);
        if !exists {
            let made = DirBuilder::new().recursive(true).mode(0o700).create(dir);
            self.blob_io(dir, made)?;
            let temp = dir.join(format!(
                ".tmp-{}-{}",
                std::process::id(),
                TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            let written = write_synced(&temp, bytes).and_then(|()| fs::rename(&temp, &path));
            if let Err(error) = written {
                let _ = fs::remove_file(&temp);
                return self.blob_io(&path, Err(error));
            }
            // Make the rename durable.
            let synced = File::open(dir).and_then(|d| d.sync_all());
            self.blob_io(dir, synced)?;
        }
        self.exec(
            "INSERT INTO blobs (hash, bytes, mime, created_at, refs) VALUES (?1, ?2, ?3, ?4, 0)
             ON CONFLICT (hash) DO NOTHING",
            params![hash, bytes.len() as i64, mime, now_ms],
        )?;
        Ok(hash)
    }

    /// The bytes of a blob. `None` when no file has this hash. A file whose content does not match
    /// its name is an error: the store never returns bytes that it cannot vouch for.
    pub fn get_blob(&self, hash: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let path = self.blob_path(hash)?;
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(StoreError::Io { path, source }),
        };
        if blake3::hash(&bytes).to_hex().as_str() != hash {
            return Err(StoreError::BadValue(format!("the blob {hash} is damaged")));
        }
        Ok(Some(bytes))
    }

    /// The row of a blob.
    pub fn blob_info(&self, hash: &str) -> Result<Option<BlobInfo>, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT hash, bytes, mime, created_at, refs FROM blobs WHERE hash = ?1",
                [hash],
                info,
            )
            .optional()?)
    }

    /// Counts one more reference. Returns `false` when the blob is unknown.
    pub fn add_blob_ref(&self, hash: &str) -> Result<bool, StoreError> {
        Ok(self.exec("UPDATE blobs SET refs = refs + 1 WHERE hash = ?1", [hash])? > 0)
    }

    /// Counts one reference less, but never below 0. Returns `false` when the blob is unknown.
    pub fn release_blob_ref(&self, hash: &str) -> Result<bool, StoreError> {
        Ok(self.exec(
            "UPDATE blobs SET refs = max(refs - 1, 0) WHERE hash = ?1",
            [hash],
        )? > 0)
    }
}

impl Tx<'_> {
    /// Counts one more reference, in the transaction of the row that names the blob. Returns
    /// `false` when the blob is unknown.
    pub fn add_blob_ref(&self, hash: &str) -> Result<bool, StoreError> {
        self.writable()?;
        Ok(self
            .sql()
            .execute("UPDATE blobs SET refs = refs + 1 WHERE hash = ?1", [hash])?
            > 0)
    }

    /// Counts one reference less, but never below 0, in the transaction of the row that let go of
    /// the blob.
    pub fn release_blob_ref(&self, hash: &str) -> Result<bool, StoreError> {
        self.writable()?;
        Ok(self.sql().execute(
            "UPDATE blobs SET refs = max(refs - 1, 0) WHERE hash = ?1",
            [hash],
        )? > 0)
    }
}

fn info(row: &rusqlite::Row<'_>) -> rusqlite::Result<BlobInfo> {
    Ok(BlobInfo {
        hash: row.get(0)?,
        bytes: u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
        mime: row.get(2)?,
        created_at_ms: row.get(3)?,
        refs: u32::try_from(row.get::<_, i64>(4)?).unwrap_or(0),
    })
}

/// Writes a new file with mode 0600 and syncs it.
fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn temp_store(name: &str) -> (Store, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("nh-store-blobs-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("a directory");
        (Store::open(&dir.join("state.db")).expect("a store"), dir)
    }

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).expect("metadata").permissions().mode() & 0o777
    }

    #[test]
    fn bytes_go_in_and_come_out_under_their_hash() {
        let (store, dir) = temp_store("roundtrip");
        let hash = store
            .put_blob(b"a screenshot", "image/png", 5)
            .expect("a blob");
        assert_eq!(hash, blake3::hash(b"a screenshot").to_hex().to_string());
        assert_eq!(
            store.get_blob(&hash).expect("a read"),
            Some(b"a screenshot".to_vec())
        );
        let path = dir.join("blobs").join(&hash[..2]).join(&hash);
        assert!(path.is_file(), "{}", path.display());
        let info = store.blob_info(&hash).expect("a row").expect("known");
        assert_eq!(
            info,
            BlobInfo {
                hash,
                bytes: 12,
                mime: "image/png".to_owned(),
                created_at_ms: 5,
                refs: 0
            }
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn files_have_mode_0600_and_directories_0700() {
        let (store, dir) = temp_store("modes");
        let hash = store.put_blob(b"secret", "text/plain", 1).expect("a blob");
        let path = store.blob_path(&hash).expect("a path");
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(path.parent().expect("a parent")), 0o700);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_same_bytes_are_one_blob() {
        let (store, dir) = temp_store("dedupe");
        let a = store.put_blob(b"same", "text/plain", 1).expect("a blob");
        let b = store.put_blob(b"same", "text/other", 9).expect("a blob");
        assert_eq!(a, b);
        let info = store.blob_info(&a).expect("a row").expect("known");
        assert_eq!(
            (info.mime.as_str(), info.created_at_ms),
            ("text/plain", 1),
            "the first row stays"
        );
        let count: i64 = store
            .conn
            .query_row("SELECT count(*) FROM blobs", [], |r| r.get(0))
            .expect("a count");
        assert_eq!(count, 1);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn no_temporary_file_stays() {
        let (store, dir) = temp_store("temp");
        for n in 0..5 {
            store
                .put_blob(format!("blob {n}").as_bytes(), "text/plain", 1)
                .expect("a blob");
        }
        let mut names = Vec::new();
        for entry in walk(&dir.join("blobs")) {
            names.push(
                entry
                    .file_name()
                    .expect("a name")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        assert_eq!(names.len(), 5);
        assert!(names.iter().all(|n| !n.starts_with(".tmp")), "{names:?}");
        let _ = fs::remove_dir_all(dir);
    }

    fn walk(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for entry in fs::read_dir(dir).expect("a directory").flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walk(&path));
            } else {
                out.push(path);
            }
        }
        out
    }

    #[test]
    fn an_unknown_blob_is_none() {
        let (store, dir) = temp_store("unknown");
        let hash = blake3::hash(b"nobody wrote this").to_hex().to_string();
        assert_eq!(store.get_blob(&hash).expect("a read"), None);
        assert_eq!(store.blob_info(&hash).expect("a read"), None);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_name_that_is_not_a_hash_is_refused() {
        let (store, dir) = temp_store("names");
        for bad in [
            "",
            "../state.db",
            "abc",
            &"A".repeat(64),
            &"g".repeat(64),
            &format!("../{}", "a".repeat(61)),
        ] {
            assert!(store.get_blob(bad).is_err(), "{bad:?}");
            assert!(store.blob_path(bad).is_err(), "{bad:?}");
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_damaged_file_is_an_error_and_not_bytes() {
        let (store, dir) = temp_store("damage");
        let hash = store
            .put_blob(b"original", "text/plain", 1)
            .expect("a blob");
        fs::write(store.blob_path(&hash).expect("a path"), b"changed!").expect("a write");
        let error = store.get_blob(&hash).expect_err("the check fails");
        assert!(matches!(error, StoreError::BadValue(m) if m.contains("damaged")));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_file_that_is_short_is_written_again() {
        let (store, dir) = temp_store("short");
        let hash = store
            .put_blob(b"complete content", "text/plain", 1)
            .expect("a blob");
        fs::write(store.blob_path(&hash).expect("a path"), b"comp").expect("a cut file");
        store
            .put_blob(b"complete content", "text/plain", 2)
            .expect("again");
        assert_eq!(
            store.get_blob(&hash).expect("a read"),
            Some(b"complete content".to_vec())
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn references_count_up_and_down_and_stop_at_zero() {
        let (store, dir) = temp_store("refs");
        let hash = store.put_blob(b"refd", "text/plain", 1).expect("a blob");
        assert!(store.add_blob_ref(&hash).expect("a ref"));
        assert!(store.add_blob_ref(&hash).expect("a ref"));
        assert_eq!(
            store.blob_info(&hash).expect("a row").expect("known").refs,
            2
        );
        assert!(store.release_blob_ref(&hash).expect("a release"));
        assert!(store.release_blob_ref(&hash).expect("a release"));
        assert!(store.release_blob_ref(&hash).expect("a release"));
        assert_eq!(
            store.blob_info(&hash).expect("a row").expect("known").refs,
            0
        );
        assert!(!store.add_blob_ref(&"0".repeat(64)).expect("unknown"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_blobs_stay_after_a_reopen() {
        let (store, dir) = temp_store("reopen");
        let hash = store.put_blob(b"kept", "text/plain", 1).expect("a blob");
        drop(store);
        let again = Store::open(&dir.join("state.db")).expect("a store");
        assert_eq!(
            again.get_blob(&hash).expect("a read"),
            Some(b"kept".to_vec())
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_store_in_memory_has_no_blobs() {
        let store = Store::open_in_memory().expect("a store");
        assert!(store.put_blob(b"x", "text/plain", 1).is_err());
    }

    #[test]
    fn a_degraded_store_refuses_to_write_a_blob_and_still_reads() {
        let (mut store, dir) = temp_store("degraded");
        let hash = store.put_blob(b"before", "text/plain", 1).expect("a blob");
        store.inject_fault(Some(crate::Fault::DiskFull));
        assert!(matches!(
            store.put_blob(b"after", "text/plain", 2),
            Err(StoreError::Degraded { .. })
        ));
        assert!(store.is_degraded());
        assert_eq!(
            store.get_blob(&hash).expect("a read"),
            Some(b"before".to_vec())
        );
        let missing = blake3::hash(b"after").to_hex().to_string();
        assert_eq!(
            store.get_blob(&missing).expect("a read"),
            None,
            "no file was written"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn two_writers_of_the_same_content_do_not_clash() {
        let (store, dir) = temp_store("race");
        let path = dir.join("state.db");
        let other = Store::open(&path).expect("a second store");
        let data = vec![7u8; 200_000];
        let (d1, d2) = (data.clone(), data.clone());
        let a = std::thread::spawn(move || store.put_blob(&d1, "application/octet-stream", 1));
        let b = std::thread::spawn(move || other.put_blob(&d2, "application/octet-stream", 2));
        let (a, b) = (a.join().expect("a thread"), b.join().expect("a thread"));
        assert_eq!(a.expect("a blob"), b.expect("a blob"));
        let hash = blake3::hash(&data).to_hex().to_string();
        let reader = Store::open(&path).expect("a third store");
        assert_eq!(reader.get_blob(&hash).expect("a read"), Some(data));
        let _ = fs::remove_dir_all(dir);
    }
}
