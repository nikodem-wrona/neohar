//! The facts of a folder for an execution: its git worktree, branch, `HEAD` and base, and a hash
//! of the tree. The core runs git only in a repository that the person trusts, as the Git view
//! does. In another folder, an execution has only the folder.
//!
//! Also the fallback for a turn whose `Stop` hook did not reach the core: the transcript of the
//! run tells if the turn ended, and its last message.

use std::{path::Path, sync::Arc, time::Duration};

use nh_agents::transcript::{Block, Format, Kind, parse_line};
use nh_proto::api::common::Harness;

use crate::changes::{
    git::Git,
    trust::{Access, Trusts},
};

/// The longest output of `git diff HEAD` that goes into the hash of the tree.
const DIFF_CAP: usize = 8 * 1024 * 1024;
/// The longest a git command of the facts runs.
const GIT_LIMIT: Duration = Duration::from_secs(10);
/// The longest transcript that the fallback reads.
const TRANSCRIPT_CAP: u64 = 64 * 1024 * 1024;

/// What an execution knows about its folder.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Facts {
    /// The root of the git worktree.
    pub repo: Option<String>,
    pub branch: Option<String>,
    /// The `HEAD` commit, short.
    pub head: Option<String>,
    /// The default branch of the upstream remote, else `main` or `master`.
    pub base: Option<String>,
    /// A hash of the `HEAD` commit and of the changes of the worktree.
    pub tree: Option<String>,
}

/// Reads the facts of folders.
pub(crate) struct FactsReader {
    trusts: Arc<Trusts>,
    git: Git,
}

impl FactsReader {
    pub(crate) fn new(trusts: Arc<Trusts>, base_env: &[(String, String)]) -> Self {
        Self {
            trusts,
            git: Git::new(base_env.to_vec()).with_limit(GIT_LIMIT),
        }
    }

    /// The facts of a folder. A folder outside a trusted git worktree has none.
    pub(crate) async fn facts(&self, folder: &Path) -> Facts {
        let Access::Open(place) = self.trusts.access(folder).await else {
            return Facts::default();
        };
        let root = place.root.clone();
        let run = |args: &'static [&'static str]| {
            let root = root.clone();
            async move {
                self.git
                    .run(&root, args)
                    .await
                    .ok()
                    .map(|out| out.trim().to_owned())
                    .filter(|out| !out.is_empty())
            }
        };
        let branch = run(&["symbolic-ref", "--short", "-q", "HEAD"]).await;
        let head = run(&["rev-parse", "--short", "HEAD"]).await;
        let full = run(&["rev-parse", "HEAD"]).await;
        let base = match run(&["rev-parse", "--abbrev-ref", "origin/HEAD"]).await {
            Some(remote) if remote != "origin/HEAD" => Some(remote),
            _ => {
                let mut found = None;
                for name in ["main", "master"] {
                    let reference = format!("refs/heads/{name}");
                    let args = ["rev-parse", "--verify", "--quiet", reference.as_str()];
                    if self.git.run(&root, &args).await.is_ok() {
                        found = Some(name.to_owned());
                        break;
                    }
                }
                found
            }
        };
        let tree = self.tree(&root, full.as_deref()).await;
        Facts {
            repo: Some(root.to_string_lossy().into_owned()),
            branch,
            head,
            base,
            tree,
        }
    }

    /// A hash of the `HEAD` commit, of the status and of the diff of the worktree.
    async fn tree(&self, root: &Path, head: Option<&str>) -> Option<String> {
        let status = self
            .git
            .run(
                root,
                &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
            )
            .await
            .ok()?;
        let diff = self
            .git
            .output_capped(root, &["diff".as_ref(), "HEAD".as_ref()], DIFF_CAP)
            .await
            .map(|(out, _)| out)
            .unwrap_or_default();
        let mut hasher = blake3::Hasher::new();
        hasher.update(head.unwrap_or_default().as_bytes());
        hasher.update(&[0]);
        hasher.update(status.as_bytes());
        hasher.update(&[0]);
        hasher.update(&diff);
        Some(hasher.finalize().to_hex()[..16].to_owned())
    }
}

/// What the transcript says about the last turn: whether it ended, and the last message of the
/// agent in it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LastTurn {
    pub ended: bool,
    pub message: Option<String>,
}

/// Reads the last turn of a transcript. `None` when the file cannot be read or has no prompt.
pub(crate) fn last_turn(harness: Harness, path: &Path) -> Option<LastTurn> {
    let format = match harness {
        Harness::Claude => Format::Claude,
        Harness::Codex => Format::Codex,
    };
    let size = std::fs::metadata(path).ok()?.len();
    if size > TRANSCRIPT_CAP {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    let mut seen_prompt = false;
    let mut turn = LastTurn::default();
    let mut message_id: Option<String> = None;
    let mut offset = 0u64;
    for line in text.lines() {
        for entry in parse_line(format, line, offset) {
            if entry.sidechain {
                continue;
            }
            match entry.kind {
                Kind::Prompt { .. } => {
                    seen_prompt = true;
                    turn = LastTurn::default();
                    message_id = None;
                }
                Kind::Assistant {
                    message, blocks, ..
                } => {
                    let texts: Vec<String> = blocks
                        .into_iter()
                        .filter_map(|block| match block {
                            Block::Text(text) => Some(text),
                            Block::ToolUse { .. } => None,
                        })
                        .collect();
                    if texts.is_empty() {
                        continue;
                    }
                    let text = texts.join("\n\n");
                    // Claude writes one line for each block of a message, with the same id.
                    match (&message_id, &message, &mut turn.message) {
                        (Some(before), Some(now), Some(kept)) if before == now => {
                            kept.push_str("\n\n");
                            kept.push_str(&text);
                        }
                        _ => turn.message = Some(text),
                    }
                    message_id = message;
                    turn.ended = false;
                }
                Kind::TurnEnd { .. } => turn.ended = true,
                _ => {}
            }
        }
        offset += line.len() as u64 + 1;
    }
    seen_prompt.then_some(turn)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn golden(path: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/harness-golden")
            .join(path)
    }

    #[test]
    fn the_transcript_of_claude_code_gives_the_last_message_of_a_turn_that_ended() {
        let turn = last_turn(
            Harness::Claude,
            &golden("claude/2.1.292/marker/transcripts/transcript-1.jsonl"),
        )
        .expect("a turn");
        assert!(turn.ended);
        assert_eq!(turn.message.as_deref(), Some("Hello from the mock model."));
    }

    #[test]
    fn the_transcript_of_codex_gives_the_last_turn_of_two() {
        let turn = last_turn(
            Harness::Codex,
            &golden("codex/0.163.0/reply-after-turn/transcripts/transcript-1.jsonl"),
        )
        .expect("a turn");
        assert!(turn.ended);
        assert_eq!(turn.message.as_deref(), Some("The reply arrived."));
    }

    #[test]
    fn a_turn_that_did_not_end_has_no_end_and_a_file_without_a_prompt_has_no_turn() {
        let text = std::fs::read_to_string(golden(
            "claude/2.1.292/marker/transcripts/transcript-1.jsonl",
        ))
        .expect("golden");
        let cut: String = text
            .lines()
            .filter(|line| !line.contains("turn_duration"))
            .map(|line| format!("{line}\n"))
            .collect();
        let dir = std::env::temp_dir().join(format!("nh-last-turn-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let open = dir.join("open.jsonl");
        std::fs::write(&open, cut).expect("write");
        let turn = last_turn(Harness::Claude, &open).expect("a turn");
        assert!(!turn.ended);
        let empty = dir.join("empty.jsonl");
        std::fs::write(&empty, "{\"type\":\"mode\",\"mode\":\"normal\"}\n").expect("write");
        assert_eq!(last_turn(Harness::Claude, &empty), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
