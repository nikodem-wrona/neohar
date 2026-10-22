//! Operations on the layout tree of a tab (`tree`).
//!
//! A tab shows its panes as nested splits. The tree is a [`LayoutNode`]: a pane, or a split with
//! an axis, one share for each child and the children. These functions are pure: they take a tree
//! and give a tree, with no I/O. Every operation keeps the **normal form**:
//!
//! - a split has at least two children, with one ratio for each;
//! - every ratio is a finite number above 0, and the ratios of a split add up to 1;
//! - a child that is a split has the other axis than its parent (two rows in a row are one row);
//! - every pane appears once.
//!
//! [`validate`] checks the normal form, and the property tests apply random operations and call it
//! after each one.

use std::collections::HashSet;

use nh_proto::api::{
    common::{Axis, SplitDirection},
    ids::PaneId,
    model::LayoutNode,
};
use thiserror::Error;

/// The smallest share that a call can give a child (`ratio` of a split, `set_ratios`). Shares that
/// come from repeated splits can be smaller.
pub const MIN_RATIO: f32 = 0.02;

/// How far from 1 the sum of the ratios of a split may be.
const SUM_TOLERANCE: f32 = 1e-3;

#[derive(Clone, Debug, PartialEq, Error)]
pub enum TreeError {
    #[error("the pane is not in this layout")]
    NoSuchPane,
    #[error("the pane is in this layout already")]
    DuplicatePane,
    #[error("the ratio must be above 0 and below 1, not {0}")]
    BadRatio(f32),
    #[error("there is no split at this path")]
    NoSuchSplit,
    #[error("a split of {children} panes needs {children} ratios, not {given}")]
    RatioCount { children: usize, given: usize },
    #[error("the ratios must be numbers of at least {MIN_RATIO} that add up to 1")]
    BadRatios,
}

pub fn leaf(pane: PaneId) -> LayoutNode {
    LayoutNode::Pane { pane }
}

/// The axis of a split for a direction: left and right are a row, up and down a column.
pub const fn axis_of(direction: SplitDirection) -> Axis {
    match direction {
        SplitDirection::Left | SplitDirection::Right => Axis::Row,
        SplitDirection::Up | SplitDirection::Down => Axis::Column,
    }
}

/// Whether the new pane comes after the old one (right, down) or before it (left, up).
pub const fn comes_after(direction: SplitDirection) -> bool {
    matches!(direction, SplitDirection::Right | SplitDirection::Down)
}

/// The panes of a tree, in the order of the layout (left to right, top to bottom).
pub fn panes(node: &LayoutNode) -> Vec<PaneId> {
    let mut out = Vec::new();
    collect(node, &mut out);
    out
}

fn collect(node: &LayoutNode, out: &mut Vec<PaneId>) {
    match node {
        LayoutNode::Pane { pane } => out.push(*pane),
        LayoutNode::Split { children, .. } => children.iter().for_each(|c| collect(c, out)),
    }
}

pub fn contains(node: &LayoutNode, pane: PaneId) -> bool {
    match node {
        LayoutNode::Pane { pane: found } => *found == pane,
        LayoutNode::Split { children, .. } => children.iter().any(|c| contains(c, pane)),
    }
}

/// The share of the new pane, from the `ratio` of a call.
fn share(ratio: Option<f32>) -> Result<f32, TreeError> {
    match ratio {
        None => Ok(0.5),
        Some(r) if r.is_finite() && r > 0.0 && r < 1.0 => Ok(r.clamp(MIN_RATIO, 1.0 - MIN_RATIO)),
        Some(r) => Err(TreeError::BadRatio(r)),
    }
}

/// `[old, new]` shares and children, in layout order.
fn pair(old: PaneId, new: PaneId, new_share: f32, after: bool) -> (Vec<f32>, Vec<LayoutNode>) {
    if after {
        (vec![1.0 - new_share, new_share], vec![leaf(old), leaf(new)])
    } else {
        (vec![new_share, 1.0 - new_share], vec![leaf(new), leaf(old)])
    }
}

/// Puts `new` next to `target`. The new pane takes `ratio` (default one half) of the room of
/// `target`. If the parent of `target` has the axis already, `new` joins it, and the other
/// children keep their shares.
pub fn split(
    root: &mut LayoutNode,
    target: PaneId,
    new: PaneId,
    direction: SplitDirection,
    ratio: Option<f32>,
) -> Result<(), TreeError> {
    let new_share = share(ratio)?;
    if contains(root, new) {
        return Err(TreeError::DuplicatePane);
    }
    if split_in(
        root,
        target,
        new,
        axis_of(direction),
        comes_after(direction),
        new_share,
    ) {
        Ok(())
    } else {
        Err(TreeError::NoSuchPane)
    }
}

fn split_in(
    node: &mut LayoutNode,
    target: PaneId,
    new: PaneId,
    axis: Axis,
    after: bool,
    new_share: f32,
) -> bool {
    match node {
        LayoutNode::Pane { pane } if *pane == target => {
            let (ratios, children) = pair(target, new, new_share, after);
            *node = LayoutNode::Split {
                axis,
                ratios,
                children,
            };
            true
        }
        LayoutNode::Pane { .. } => false,
        LayoutNode::Split {
            axis: own,
            ratios,
            children,
        } => {
            let direct = children
                .iter()
                .position(|c| matches!(c, LayoutNode::Pane { pane } if *pane == target));
            if let Some(at) = direct {
                if *own == axis {
                    let room = ratios[at];
                    let mine = room * new_share;
                    ratios[at] = room - mine;
                    if after {
                        ratios.insert(at + 1, mine);
                        children.insert(at + 1, leaf(new));
                    } else {
                        ratios.insert(at, mine);
                        children.insert(at, leaf(new));
                    }
                } else {
                    let (pair_ratios, pair_children) = pair(target, new, new_share, after);
                    children[at] = LayoutNode::Split {
                        axis,
                        ratios: pair_ratios,
                        children: pair_children,
                    };
                }
                return true;
            }
            children
                .iter_mut()
                .any(|c| split_in(c, target, new, axis, after, new_share))
        }
    }
}

/// Puts `new` at the edge of the whole tree. In a split with the axis, `new` joins it and takes
/// `ratio` (default: an equal share). Otherwise the tree and `new` become the two children of a
/// new split.
pub fn insert_at_root(
    root: &mut LayoutNode,
    new: PaneId,
    direction: SplitDirection,
    ratio: Option<f32>,
) -> Result<(), TreeError> {
    if contains(root, new) {
        return Err(TreeError::DuplicatePane);
    }
    let axis = axis_of(direction);
    let after = comes_after(direction);
    if let LayoutNode::Split {
        axis: own,
        ratios,
        children,
    } = root
        && *own == axis
    {
        let new_share = match ratio {
            Some(_) => share(ratio)?,
            None => 1.0 / (children.len() as f32 + 1.0),
        };
        for r in ratios.iter_mut() {
            *r *= 1.0 - new_share;
        }
        if after {
            ratios.push(new_share);
            children.push(leaf(new));
        } else {
            ratios.insert(0, new_share);
            children.insert(0, leaf(new));
        }
        return Ok(());
    }
    let new_share = share(ratio)?;
    let old = std::mem::replace(root, leaf(new));
    let (ratios, children) = if after {
        (vec![1.0 - new_share, new_share], vec![old, leaf(new)])
    } else {
        (vec![new_share, 1.0 - new_share], vec![leaf(new), old])
    };
    *root = LayoutNode::Split {
        axis,
        ratios,
        children,
    };
    Ok(())
}

/// Takes a pane out. The room of the pane goes to its siblings in proportion to their shares. A
/// split with one child left is replaced by the child. `None` means that the pane was the only
/// one, and the tree is gone.
pub fn remove(root: LayoutNode, pane: PaneId) -> Result<Option<LayoutNode>, TreeError> {
    if !contains(&root, pane) {
        return Err(TreeError::NoSuchPane);
    }
    Ok(remove_in(root, pane).map(normalize))
}

fn remove_in(node: LayoutNode, pane: PaneId) -> Option<LayoutNode> {
    match node {
        LayoutNode::Pane { pane: found } if found == pane => None,
        leaf @ LayoutNode::Pane { .. } => Some(leaf),
        LayoutNode::Split {
            axis,
            ratios,
            children,
        } => {
            let mut kept: Vec<(f32, LayoutNode)> = ratios
                .into_iter()
                .zip(children)
                .filter_map(|(ratio, child)| remove_in(child, pane).map(|c| (ratio, c)))
                .collect();
            match kept.len() {
                0 => None,
                1 => kept.pop().map(|(_, child)| child),
                _ => {
                    let total: f32 = kept.iter().map(|(r, _)| r).sum();
                    let (ratios, children) = kept.into_iter().map(|(r, c)| (r / total, c)).unzip();
                    Some(LayoutNode::Split {
                        axis,
                        ratios,
                        children,
                    })
                }
            }
        }
    }
}

/// Brings a tree to the normal form: a split inside a split with the same axis is merged into its
/// parent, a split with one child is replaced by the child, and the ratios add up to 1.
pub fn normalize(node: LayoutNode) -> LayoutNode {
    let LayoutNode::Split {
        axis,
        ratios,
        children,
    } = node
    else {
        return node;
    };
    let mut out_ratios = Vec::with_capacity(children.len());
    let mut out_children = Vec::with_capacity(children.len());
    for (ratio, child) in ratios.into_iter().zip(children) {
        match normalize(child) {
            LayoutNode::Split {
                axis: inner_axis,
                ratios: inner_ratios,
                children: inner_children,
            } if inner_axis == axis => {
                for (r, c) in inner_ratios.into_iter().zip(inner_children) {
                    out_ratios.push(ratio * r);
                    out_children.push(c);
                }
            }
            other => {
                out_ratios.push(ratio);
                out_children.push(other);
            }
        }
    }
    if out_children.len() == 1 {
        return out_children.remove(0);
    }
    let total: f32 = out_ratios.iter().sum();
    if total > 0.0 {
        out_ratios.iter_mut().for_each(|r| *r /= total);
    }
    LayoutNode::Split {
        axis,
        ratios: out_ratios,
        children: out_children,
    }
}

/// The node at a path of child indices from the root. `[]` is the root.
pub fn node_at<'a>(root: &'a mut LayoutNode, path: &[u32]) -> Option<&'a mut LayoutNode> {
    let mut node = root;
    for index in path {
        match node {
            LayoutNode::Split { children, .. } => {
                node = children.get_mut(usize::try_from(*index).ok()?)?;
            }
            LayoutNode::Pane { .. } => return None,
        }
    }
    Some(node)
}

/// Sets the ratios of the split at `path`. They must be as many as the children, at least
/// [`MIN_RATIO`], and add up to 1 (a sum that is off by less than 1 percent is scaled to 1).
pub fn set_ratios(
    root: &mut LayoutNode,
    path: &[u32],
    new_ratios: &[f32],
) -> Result<(), TreeError> {
    let Some(LayoutNode::Split {
        ratios, children, ..
    }) = node_at(root, path)
    else {
        return Err(TreeError::NoSuchSplit);
    };
    if new_ratios.len() != children.len() {
        return Err(TreeError::RatioCount {
            children: children.len(),
            given: new_ratios.len(),
        });
    }
    if new_ratios.iter().any(|r| !r.is_finite() || *r < MIN_RATIO) {
        return Err(TreeError::BadRatios);
    }
    let total: f32 = new_ratios.iter().sum();
    if (total - 1.0).abs() > 0.01 {
        return Err(TreeError::BadRatios);
    }
    *ratios = new_ratios.iter().map(|r| r / total).collect();
    Ok(())
}

/// Gives the children of the split at `path` equal shares, and does the same for every split
/// below it. A path that names a pane changes nothing.
pub fn equalize(root: &mut LayoutNode, path: &[u32]) -> Result<(), TreeError> {
    let node = node_at(root, path).ok_or(TreeError::NoSuchSplit)?;
    equalize_all(node);
    Ok(())
}

fn equalize_all(node: &mut LayoutNode) {
    if let LayoutNode::Split {
        ratios, children, ..
    } = node
    {
        let each = 1.0 / children.len() as f32;
        ratios.iter_mut().for_each(|r| *r = each);
        children.iter_mut().for_each(equalize_all);
    }
}

/// Checks the normal form.
pub fn validate(root: &LayoutNode) -> Result<(), String> {
    let mut seen = HashSet::new();
    check(root, None, &mut seen)
}

fn check(
    node: &LayoutNode,
    parent_axis: Option<Axis>,
    seen: &mut HashSet<PaneId>,
) -> Result<(), String> {
    match node {
        LayoutNode::Pane { pane } => {
            if seen.insert(*pane) {
                Ok(())
            } else {
                Err(format!("pane {pane} appears twice"))
            }
        }
        LayoutNode::Split {
            axis,
            ratios,
            children,
        } => {
            if children.len() < 2 {
                return Err(format!("a split with {} children", children.len()));
            }
            if ratios.len() != children.len() {
                return Err(format!(
                    "{} ratios for {} children",
                    ratios.len(),
                    children.len()
                ));
            }
            if ratios.iter().any(|r| !r.is_finite() || *r <= 0.0) {
                return Err(format!("a ratio that is not above 0: {ratios:?}"));
            }
            let total: f32 = ratios.iter().sum();
            if (total - 1.0).abs() > SUM_TOLERANCE {
                return Err(format!("the ratios add up to {total}"));
            }
            if parent_axis == Some(*axis) {
                return Err("a split inside a split with the same axis".to_owned());
            }
            children
                .iter()
                .try_for_each(|c| check(c, Some(*axis), seen))
        }
    }
}

#[cfg(test)]
mod tests;
