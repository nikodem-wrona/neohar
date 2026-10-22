//! The file that holds a sequence of frames.

use std::fmt;

const MAGIC: &[u8; 8] = b"NHFRAMES";
const VERSION: u32 = 1;

/// One frame, as the core sends it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// The display frame that the frame belongs to.
    pub tick: u32,
    pub pane: u32,
    pub bytes: Vec<u8>,
}

/// A whole file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Container {
    pub panes: u32,
    pub cols: u32,
    pub rows: u32,
    pub records: Vec<Record>,
}

/// The file is not a sequence of frames.
#[derive(Debug, PartialEq, Eq)]
pub struct Malformed(pub &'static str);

impl fmt::Display for Malformed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not a frame file: {}", self.0)
    }
}

impl std::error::Error for Malformed {}

/// The bytes of a file.
pub fn write(container: &Container) -> Vec<u8> {
    let body: usize = container.records.iter().map(|r| 12 + r.bytes.len()).sum();
    let mut out = Vec::with_capacity(28 + body);
    out.extend_from_slice(MAGIC);
    for value in [
        VERSION,
        container.panes,
        container.cols,
        container.rows,
        container.records.len() as u32,
    ] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    for record in &container.records {
        out.extend_from_slice(&record.tick.to_le_bytes());
        out.extend_from_slice(&record.pane.to_le_bytes());
        out.extend_from_slice(&(record.bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&record.bytes);
    }
    out
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Malformed> {
        let end = self
            .at
            .checked_add(n)
            .filter(|&end| end <= self.bytes.len())
            .ok_or(Malformed("the file ends early"))?;
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    fn u32(&mut self) -> Result<u32, Malformed> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

/// Reads a file.
pub fn read(bytes: &[u8]) -> Result<Container, Malformed> {
    let mut r = Reader { bytes, at: 0 };
    if r.take(8)? != MAGIC {
        return Err(Malformed("the magic is wrong"));
    }
    if r.u32()? != VERSION {
        return Err(Malformed("the version is not known"));
    }
    let (panes, cols, rows, count) = (r.u32()?, r.u32()?, r.u32()?, r.u32()?);
    let mut records = Vec::new();
    for _ in 0..count {
        let (tick, pane, len) = (r.u32()?, r.u32()?, r.u32()? as usize);
        records.push(Record {
            tick,
            pane,
            bytes: r.take(len)?.to_vec(),
        });
    }
    Ok(Container {
        panes,
        cols,
        rows,
        records,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Container {
        Container {
            panes: 2,
            cols: 80,
            rows: 24,
            records: vec![
                Record {
                    tick: 0,
                    pane: 1,
                    bytes: vec![1, 2, 3],
                },
                Record {
                    tick: 0,
                    pane: 2,
                    bytes: vec![],
                },
                Record {
                    tick: 1,
                    pane: 1,
                    bytes: vec![9; 300],
                },
            ],
        }
    }

    #[test]
    fn a_file_reads_back_as_written() {
        assert_eq!(read(&write(&sample())), Ok(sample()));
    }

    #[test]
    fn a_file_that_is_cut_or_wrong_is_refused() {
        let bytes = write(&sample());
        for cut in 0..bytes.len() {
            assert!(read(&bytes[..cut]).is_err(), "cut at {cut}");
        }
        let mut wrong = bytes.clone();
        wrong[0] = b'X';
        assert_eq!(read(&wrong), Err(Malformed("the magic is wrong")));
        let mut version = bytes;
        version[8] = 9;
        assert_eq!(read(&version), Err(Malformed("the version is not known")));
    }
}
