//! Rich content. `nh-rich` prepares it in Rust, and the UI draws it without any
//! formatting logic. All offsets are UTF-16 code units, because JavaScript slices strings that way.

use super::common::{Risk, wire};

wire! {
    /// Text with highlighted ranges.
    pub struct Spans {
        pub text: String,
        /// `[start, end, class]`, in UTF-16 code units.
        pub spans: Vec<(u32, u32, String)>,
    }

    #[serde(rename_all = "snake_case")]
    #[derive(Copy, Eq, Hash)]
    pub enum FileStatus {
        Modified,
        Added,
        Deleted,
        Renamed,
    }

    #[serde(rename_all = "snake_case")]
    #[derive(Copy, Eq, Hash)]
    pub enum LineKind {
        Ctx,
        Add,
        Del,
    }

    pub struct DiffLine {
        pub kind: LineKind,
        /// The line number in the old file. A context line and a deleted line have one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub old: Option<u32>,
        /// The line number in the new file. A context line and an added line have one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub new: Option<u32>,
        pub text: Spans,
        /// The changed ranges inside the line, in UTF-16 code units of `text`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub intra: Option<Vec<(u32, u32)>>,
    }

    pub struct Hunk {
        pub header: String,
        pub lines: Vec<DiffLine>,
    }

    #[derive(Copy, Eq)]
    pub struct DiffStats {
        pub add: u32,
        pub del: u32,
    }

    pub struct FileDiff {
        pub path: String,
        pub status: FileStatus,
        pub hunks: Vec<Hunk>,
        pub stats: DiffStats,
        /// The diff of this file was cut at 2,000 lines. The UI offers "open full".
        #[serde(default)]
        pub truncated: bool,
    }

    pub struct Diff {
        pub files: Vec<FileDiff>,
    }

    /// Sanitized HTML (`ammonia`). The UI inserts it only through the `RichHtml` component.
    pub struct Markdown {
        pub html: String,
        /// The text was cut at the cap (256 KB for a plan). The UI offers "open full".
        #[serde(default)]
        pub truncated: bool,
    }

    /// A shell command, for an approval card.
    pub struct Command {
        pub argv: Vec<String>,
        pub display: Spans,
        pub cwd: String,
        pub risk: Risk,
        pub reasons: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub policy_hint: Option<String>,
    }

    /// A screenshot of a preview.
    pub struct Screenshot {
        /// The hash of the blob. `blobUrl(hash)` makes the URL.
        pub blob: String,
        pub width: u32,
        pub height: u32,
        pub env_label: String,
    }
}
