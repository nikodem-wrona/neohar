//! The client device of this app: a ULID made at the first launch, and the host
//! name. The app keeps the id in `<data>/device-id`, with mode 0600, and it gives both to the core
//! as the device of its host ([`crate::host::InProcessHost`]).

use std::{
    fs::OpenOptions,
    io::{self, Write as _},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

use ulid::Ulid;

use crate::host::ClientInfo;

/// The name of the file of the device id in the data directory of the session.
pub const FILE: &str = "device-id";

/// The file of the device id in `data_dir`.
pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE)
}

/// The device id in `data_dir`. The first call makes one and writes it. A file that holds no
/// ULID is replaced, because a device without an id cannot own a window.
pub fn load_or_create(data_dir: &Path) -> io::Result<Ulid> {
    let file = path(data_dir);
    match std::fs::read_to_string(&file) {
        Ok(text) => {
            if let Ok(id) = text.trim().parse::<Ulid>() {
                return Ok(id);
            }
            tracing::warn!(path = %file.display(), "the device id is not a ULID, so a new one is made");
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    std::fs::create_dir_all(data_dir)?;
    let id = crate::ids::new_ulid().map_err(|error| io::Error::other(error.message))?;
    let temp = data_dir.join(format!("{FILE}.{}.tmp", std::process::id()));
    {
        let mut out = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temp)?;
        writeln!(out, "{id}")?;
        out.sync_all()?;
    }
    std::fs::rename(&temp, &file)?;
    Ok(id)
}

/// The name of this device: the host name of the machine.
pub fn name() -> String {
    rustix::system::uname()
        .nodename()
        .to_str()
        .ok()
        .filter(|name| !name.is_empty())
        .map_or_else(|| "unknown".to_owned(), str::to_owned)
}

/// The device of this app: the id in `data_dir` and the host name.
pub fn this_device(data_dir: &Path) -> io::Result<ClientInfo> {
    Ok(ClientInfo {
        device: load_or_create(data_dir)?,
        name: name(),
    })
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nh-device-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_first_launch_makes_the_id_and_the_next_one_reads_it() {
        let dir = dir("keep");
        let first = load_or_create(&dir).expect("an id");
        let second = load_or_create(&dir).expect("the same id");
        assert_eq!(first, second);
        let mode = std::fs::metadata(path(&dir))
            .expect("the file")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_without_a_ulid_gets_a_new_id() {
        let dir = dir("broken");
        std::fs::create_dir_all(&dir).expect("a directory");
        std::fs::write(path(&dir), "not an id").expect("a file");
        let id = load_or_create(&dir).expect("a new id");
        assert_eq!(load_or_create(&dir).expect("the same id"), id);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_device_has_a_name() {
        assert!(!name().is_empty());
    }
}
