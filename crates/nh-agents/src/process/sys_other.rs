//! The walk reads nothing on a system that is neither macOS nor Linux. neohar runs on those two.

use std::{collections::BTreeMap, path::PathBuf};

use super::{Args, Basic, Kin};

pub(super) fn basic(_pid: i32) -> Option<Basic> {
    None
}

pub(super) fn exe(_pid: i32) -> Option<PathBuf> {
    None
}

pub(super) fn cwd(_pid: i32) -> Option<PathBuf> {
    None
}

pub(super) fn children(_pid: i32) -> Vec<i32> {
    Vec::new()
}

pub(super) fn args(_pid: i32, _want_env: bool) -> Option<Args> {
    None
}

pub(super) fn all() -> Vec<Kin> {
    Vec::new()
}
