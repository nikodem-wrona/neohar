//! A writer of zip files with stored entries (no compression).
//!
//! The debug bundle is a few hundred kilobytes of text, and a bug report has to open on every
//! system, so the writer makes the plainest zip: each file as a stored entry with its CRC-32. It
//! needs no dependency.

use std::io::{self, Write};

/// The table of the CRC-32 (the polynomial of zip and PNG), made at compile time.
const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
};

/// The CRC-32 of the bytes.
pub(crate) fn crc32(bytes: &[u8]) -> u32 {
    !bytes.iter().fold(!0u32, |crc, byte| {
        CRC_TABLE[usize::from((crc as u8) ^ byte)] ^ (crc >> 8)
    })
}

/// The date 1980-01-01 00:00, which zip can hold. A bundle says when it was made in its own files.
const DOS_TIME: u16 = 0;
const DOS_DATE: u16 = (1 << 5) | 1;

struct Entry {
    name: String,
    crc: u32,
    size: u32,
    offset: u32,
}

pub(crate) struct ZipWriter<W: Write> {
    out: W,
    entries: Vec<Entry>,
    written: u64,
}

fn too_big(what: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{what} is too big for a zip"),
    )
}

impl<W: Write> ZipWriter<W> {
    pub(crate) fn new(out: W) -> Self {
        Self {
            out,
            entries: Vec::new(),
            written: 0,
        }
    }

    fn put(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.out.write_all(bytes)?;
        self.written += bytes.len() as u64;
        Ok(())
    }

    /// Adds a file.
    pub(crate) fn add(&mut self, name: &str, data: &[u8]) -> io::Result<()> {
        let size = u32::try_from(data.len()).map_err(|_| too_big("a file"))?;
        let offset = u32::try_from(self.written).map_err(|_| too_big("the archive"))?;
        let name_len = u16::try_from(name.len()).map_err(|_| too_big("a name"))?;
        if self.entries.len() >= usize::from(u16::MAX) {
            return Err(too_big("the list of files"));
        }
        let crc = crc32(data);
        let mut header = Vec::with_capacity(30 + name.len());
        header.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        header.extend_from_slice(&20u16.to_le_bytes()); // version needed
        header.extend_from_slice(&0x0800u16.to_le_bytes()); // names are UTF-8
        header.extend_from_slice(&0u16.to_le_bytes()); // stored
        header.extend_from_slice(&DOS_TIME.to_le_bytes());
        header.extend_from_slice(&DOS_DATE.to_le_bytes());
        header.extend_from_slice(&crc.to_le_bytes());
        header.extend_from_slice(&size.to_le_bytes());
        header.extend_from_slice(&size.to_le_bytes());
        header.extend_from_slice(&name_len.to_le_bytes());
        header.extend_from_slice(&0u16.to_le_bytes()); // no extra field
        header.extend_from_slice(name.as_bytes());
        self.put(&header)?;
        self.put(data)?;
        self.entries.push(Entry {
            name: name.to_owned(),
            crc,
            size,
            offset,
        });
        Ok(())
    }

    /// Writes the central directory. Returns the writer and the number of bytes written.
    pub(crate) fn finish(mut self) -> io::Result<(W, u64)> {
        let start = u32::try_from(self.written).map_err(|_| too_big("the archive"))?;
        let mut directory = Vec::new();
        for entry in &self.entries {
            directory.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            directory.extend_from_slice(&((3u16 << 8) | 20).to_le_bytes()); // made by: Unix
            directory.extend_from_slice(&20u16.to_le_bytes());
            directory.extend_from_slice(&0x0800u16.to_le_bytes());
            directory.extend_from_slice(&0u16.to_le_bytes());
            directory.extend_from_slice(&DOS_TIME.to_le_bytes());
            directory.extend_from_slice(&DOS_DATE.to_le_bytes());
            directory.extend_from_slice(&entry.crc.to_le_bytes());
            directory.extend_from_slice(&entry.size.to_le_bytes());
            directory.extend_from_slice(&entry.size.to_le_bytes());
            directory.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
            directory.extend_from_slice(&0u16.to_le_bytes()); // extra
            directory.extend_from_slice(&0u16.to_le_bytes()); // comment
            directory.extend_from_slice(&0u16.to_le_bytes()); // disk
            directory.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
            directory.extend_from_slice(&(0o100_600u32 << 16).to_le_bytes()); // a file, 0600
            directory.extend_from_slice(&entry.offset.to_le_bytes());
            directory.extend_from_slice(entry.name.as_bytes());
        }
        let size = u32::try_from(directory.len()).map_err(|_| too_big("the directory"))?;
        self.put(&directory)?;
        let count = self.entries.len() as u16;
        let mut end = Vec::with_capacity(22);
        end.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        end.extend_from_slice(&0u16.to_le_bytes());
        end.extend_from_slice(&0u16.to_le_bytes());
        end.extend_from_slice(&count.to_le_bytes());
        end.extend_from_slice(&count.to_le_bytes());
        end.extend_from_slice(&size.to_le_bytes());
        end.extend_from_slice(&start.to_le_bytes());
        end.extend_from_slice(&0u16.to_le_bytes());
        self.put(&end)?;
        self.out.flush()?;
        Ok((self.out, self.written))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u16_at(bytes: &[u8], at: usize) -> usize {
        usize::from(u16::from_le_bytes([bytes[at], bytes[at + 1]]))
    }

    fn u32_at(bytes: &[u8], at: usize) -> usize {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize
    }

    #[test]
    fn the_crc_matches_the_known_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn a_zip_reads_back_through_its_directory() {
        let mut zip = ZipWriter::new(Vec::new());
        zip.add("a.txt", b"hello").expect("a file");
        zip.add("dir/b.json", b"{\"n\":1}").expect("a file");
        let (bytes, written) = zip.finish().expect("an archive");
        assert_eq!(written as usize, bytes.len());
        // The end of the central directory is the last 22 bytes.
        let end = bytes.len() - 22;
        assert_eq!(u32_at(&bytes, end), 0x0605_4b50);
        assert_eq!(u16_at(&bytes, end + 10), 2);
        let mut at = u32_at(&bytes, end + 16);
        let mut found = Vec::new();
        for _ in 0..2 {
            assert_eq!(u32_at(&bytes, at), 0x0201_4b50);
            let (crc, size) = (u32_at(&bytes, at + 16), u32_at(&bytes, at + 24));
            let (name_len, offset) = (u16_at(&bytes, at + 28), u32_at(&bytes, at + 42));
            let name =
                String::from_utf8(bytes[at + 46..at + 46 + name_len].to_vec()).expect("a name");
            // The local header: 30 bytes and the name, then the data.
            let local_name = u16_at(&bytes, offset + 26);
            let data = &bytes[offset + 30 + local_name..offset + 30 + local_name + size];
            assert_eq!(crc32(data) as usize, crc);
            found.push((name, data.to_vec()));
            at += 46 + name_len;
        }
        assert_eq!(found[0], ("a.txt".to_owned(), b"hello".to_vec()));
        assert_eq!(found[1].0, "dir/b.json");
    }
}
