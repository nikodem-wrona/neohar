use proptest::prelude::*;
use ulid::Ulid;

use super::*;

fn pane(n: u128) -> PaneId {
    PaneId::new(Ulid::from(n))
}

fn p(n: u128) -> LayoutNode {
    leaf(pane(n))
}

fn row(ratios: &[f32], children: Vec<LayoutNode>) -> LayoutNode {
    LayoutNode::Split {
        axis: Axis::Row,
        ratios: ratios.to_vec(),
        children,
    }
}

fn column(ratios: &[f32], children: Vec<LayoutNode>) -> LayoutNode {
    LayoutNode::Split {
        axis: Axis::Column,
        ratios: ratios.to_vec(),
        children,
    }
}

fn near(a: &LayoutNode, b: &LayoutNode) -> bool {
    match (a, b) {
        (LayoutNode::Pane { pane: x }, LayoutNode::Pane { pane: y }) => x == y,
        (
            LayoutNode::Split {
                axis: ax,
                ratios: rx,
                children: cx,
            },
            LayoutNode::Split {
                axis: ay,
                ratios: ry,
                children: cy,
            },
        ) => {
            ax == ay
                && rx.len() == ry.len()
                && rx.iter().zip(ry).all(|(x, y)| (x - y).abs() < 1e-5)
                && cx.len() == cy.len()
                && cx.iter().zip(cy).all(|(x, y)| near(x, y))
        }
        _ => false,
    }
}

#[track_caller]
fn assert_tree(actual: &LayoutNode, expected: &LayoutNode) {
    assert!(near(actual, expected), "{actual:#?}\nis not\n{expected:#?}");
    validate(actual).expect("the normal form");
}

#[test]
fn splitting_a_single_pane_makes_a_split_on_the_axis_of_the_direction() {
    for (direction, expected) in [
        (SplitDirection::Right, row(&[0.5, 0.5], vec![p(1), p(2)])),
        (SplitDirection::Left, row(&[0.5, 0.5], vec![p(2), p(1)])),
        (SplitDirection::Down, column(&[0.5, 0.5], vec![p(1), p(2)])),
        (SplitDirection::Up, column(&[0.5, 0.5], vec![p(2), p(1)])),
    ] {
        let mut tree = p(1);
        split(&mut tree, pane(1), pane(2), direction, None).expect("a split");
        assert_tree(&tree, &expected);
    }
}

#[test]
fn the_new_pane_takes_the_ratio_that_the_call_gave() {
    let mut tree = p(1);
    split(
        &mut tree,
        pane(1),
        pane(2),
        SplitDirection::Right,
        Some(0.3),
    )
    .expect("a split");
    assert_tree(&tree, &row(&[0.7, 0.3], vec![p(1), p(2)]));
    let mut tree = p(1);
    split(&mut tree, pane(1), pane(2), SplitDirection::Left, Some(0.3)).expect("a split");
    assert_tree(&tree, &row(&[0.3, 0.7], vec![p(2), p(1)]));
}

#[test]
fn a_split_on_the_axis_of_the_parent_joins_the_parent_and_keeps_the_other_shares() {
    let mut tree = row(&[0.6, 0.4], vec![p(1), p(2)]);
    split(&mut tree, pane(1), pane(3), SplitDirection::Right, None).expect("a split");
    // The new pane takes half of the room of pane 1.
    assert_tree(&tree, &row(&[0.3, 0.3, 0.4], vec![p(1), p(3), p(2)]));
    split(
        &mut tree,
        pane(2),
        pane(4),
        SplitDirection::Left,
        Some(0.25),
    )
    .expect("a split");
    assert_tree(
        &tree,
        &row(&[0.3, 0.3, 0.1, 0.3], vec![p(1), p(3), p(4), p(2)]),
    );
}

#[test]
fn a_split_on_the_other_axis_makes_a_nested_split() {
    let mut tree = row(&[0.5, 0.5], vec![p(1), p(2)]);
    split(&mut tree, pane(2), pane(3), SplitDirection::Down, None).expect("a split");
    assert_tree(
        &tree,
        &row(
            &[0.5, 0.5],
            vec![p(1), column(&[0.5, 0.5], vec![p(2), p(3)])],
        ),
    );
}

#[test]
fn a_split_goes_to_the_pane_wherever_it_is() {
    let mut tree = row(
        &[0.5, 0.5],
        vec![p(1), column(&[0.5, 0.5], vec![p(2), p(3)])],
    );
    split(&mut tree, pane(3), pane(4), SplitDirection::Right, None).expect("a split");
    assert_eq!(panes(&tree), [pane(1), pane(2), pane(3), pane(4)]);
    validate(&tree).expect("the normal form");
}

#[test]
fn a_split_refuses_a_bad_ratio_a_known_new_pane_and_an_unknown_target() {
    let mut tree = row(&[0.5, 0.5], vec![p(1), p(2)]);
    let before = tree.clone();
    for ratio in [0.0, 1.0, -0.5, 1.5, f32::NAN, f32::INFINITY] {
        let error = split(
            &mut tree,
            pane(1),
            pane(3),
            SplitDirection::Right,
            Some(ratio),
        )
        .expect_err("a bad ratio");
        assert!(matches!(error, TreeError::BadRatio(_)), "{ratio}");
    }
    assert_eq!(
        split(&mut tree, pane(1), pane(2), SplitDirection::Right, None),
        Err(TreeError::DuplicatePane)
    );
    assert_eq!(
        split(&mut tree, pane(9), pane(3), SplitDirection::Right, None),
        Err(TreeError::NoSuchPane)
    );
    assert_eq!(tree, before, "a refused split changes nothing");
}

#[test]
fn a_ratio_close_to_the_edge_is_kept_off_it() {
    let mut tree = p(1);
    split(
        &mut tree,
        pane(1),
        pane(2),
        SplitDirection::Right,
        Some(0.001),
    )
    .expect("a split");
    let LayoutNode::Split { ratios, .. } = &tree else {
        panic!("a split")
    };
    assert!((ratios[1] - MIN_RATIO).abs() < 1e-6, "{ratios:?}");
}

#[test]
fn removing_the_only_pane_leaves_no_tree() {
    assert_eq!(remove(p(1), pane(1)), Ok(None));
    assert_eq!(remove(p(1), pane(2)), Err(TreeError::NoSuchPane));
}

#[test]
fn removing_one_of_two_leaves_the_other_alone() {
    let tree = row(&[0.3, 0.7], vec![p(1), p(2)]);
    assert_tree(
        &remove(tree, pane(1)).expect("a removal").expect("a tree"),
        &p(2),
    );
}

#[test]
fn the_room_of_a_removed_pane_goes_to_the_others_in_proportion() {
    let tree = row(&[0.2, 0.3, 0.5], vec![p(1), p(2), p(3)]);
    // Without pane 2: 0.2 and 0.5 share 0.7.
    let kept = remove(tree, pane(2)).expect("a removal").expect("a tree");
    assert_tree(&kept, &row(&[0.2 / 0.7, 0.5 / 0.7], vec![p(1), p(3)]));
}

#[test]
fn a_split_that_is_left_with_one_child_merges_into_a_parent_with_the_same_axis() {
    // Row [1, Column [2, Row [3, 4]]]: without pane 2 the column is a row [3, 4], and it merges.
    let tree = row(
        &[0.5, 0.5],
        vec![
            p(1),
            column(&[0.5, 0.5], vec![p(2), row(&[0.4, 0.6], vec![p(3), p(4)])]),
        ],
    );
    let kept = remove(tree, pane(2)).expect("a removal").expect("a tree");
    assert_tree(&kept, &row(&[0.5, 0.2, 0.3], vec![p(1), p(3), p(4)]));
}

#[test]
fn removing_a_pane_in_a_nested_split_keeps_the_shares_above() {
    let tree = row(
        &[0.4, 0.6],
        vec![p(1), column(&[0.25, 0.25, 0.5], vec![p(2), p(3), p(4)])],
    );
    let kept = remove(tree, pane(3)).expect("a removal").expect("a tree");
    assert_tree(
        &kept,
        &row(
            &[0.4, 0.6],
            vec![p(1), column(&[1.0 / 3.0, 2.0 / 3.0], vec![p(2), p(4)])],
        ),
    );
}

#[test]
fn ratios_can_be_set_at_the_root_and_in_a_nested_split() {
    let mut tree = row(
        &[0.5, 0.5],
        vec![p(1), column(&[0.5, 0.5], vec![p(2), p(3)])],
    );
    set_ratios(&mut tree, &[], &[0.25, 0.75]).expect("the root");
    set_ratios(&mut tree, &[1], &[0.6, 0.4]).expect("a nested split");
    assert_tree(
        &tree,
        &row(
            &[0.25, 0.75],
            vec![p(1), column(&[0.6, 0.4], vec![p(2), p(3)])],
        ),
    );
    // A sum that is a little off is scaled.
    set_ratios(&mut tree, &[], &[0.3, 0.705]).expect("a small error");
    validate(&tree).expect("the normal form");
}

#[test]
fn bad_ratios_and_bad_paths_are_refused_and_change_nothing() {
    let mut tree = row(
        &[0.5, 0.5],
        vec![p(1), column(&[0.5, 0.5], vec![p(2), p(3)])],
    );
    let before = tree.clone();
    assert_eq!(
        set_ratios(&mut tree, &[0], &[1.0]),
        Err(TreeError::NoSuchSplit),
        "a pane"
    );
    assert_eq!(
        set_ratios(&mut tree, &[5], &[0.5, 0.5]),
        Err(TreeError::NoSuchSplit)
    );
    assert_eq!(
        set_ratios(&mut tree, &[1, 0], &[0.5, 0.5]),
        Err(TreeError::NoSuchSplit)
    );
    assert_eq!(
        set_ratios(&mut tree, &[], &[1.0]),
        Err(TreeError::RatioCount {
            children: 2,
            given: 1
        })
    );
    for bad in [
        [0.5, 0.6],
        [0.0, 1.0],
        [-0.2, 1.2],
        [f32::NAN, 0.5],
        [0.01, 0.99],
        [0.3, 0.3],
    ] {
        assert_eq!(
            set_ratios(&mut tree, &[], &bad),
            Err(TreeError::BadRatios),
            "{bad:?}"
        );
    }
    assert_eq!(tree, before);
}

#[test]
fn equalize_sets_equal_shares_for_a_node_and_everything_below() {
    let mut tree = row(
        &[0.1, 0.9],
        vec![p(1), column(&[0.2, 0.3, 0.5], vec![p(2), p(3), p(4)])],
    );
    equalize(&mut tree, &[1]).expect("a nested split");
    assert_tree(
        &tree,
        &row(
            &[0.1, 0.9],
            vec![p(1), column(&[1.0 / 3.0; 3], vec![p(2), p(3), p(4)])],
        ),
    );
    equalize(&mut tree, &[]).expect("the root");
    assert_tree(
        &tree,
        &row(
            &[0.5, 0.5],
            vec![p(1), column(&[1.0 / 3.0; 3], vec![p(2), p(3), p(4)])],
        ),
    );
    // A pane has nothing to equalize, and a path that leads nowhere is an error.
    equalize(&mut tree, &[0]).expect("a pane");
    assert_eq!(equalize(&mut tree, &[7]), Err(TreeError::NoSuchSplit));
}

#[test]
fn a_pane_at_the_root_joins_a_split_on_its_axis_or_wraps_the_tree() {
    let mut tree = row(&[0.5, 0.5], vec![p(1), p(2)]);
    insert_at_root(&mut tree, pane(3), SplitDirection::Right, None).expect("an insert");
    assert_tree(
        &tree,
        &row(&[1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0], vec![p(1), p(2), p(3)]),
    );
    insert_at_root(&mut tree, pane(4), SplitDirection::Left, Some(0.4)).expect("an insert");
    assert_tree(
        &tree,
        &row(&[0.4, 0.2, 0.2, 0.2], vec![p(4), p(1), p(2), p(3)]),
    );
    // The other axis wraps the whole tree.
    insert_at_root(&mut tree, pane(5), SplitDirection::Down, None).expect("an insert");
    let LayoutNode::Split { axis, children, .. } = &tree else {
        panic!("a split")
    };
    assert_eq!((*axis, children.len()), (Axis::Column, 2));
    assert_eq!(panes(&tree).last(), Some(&pane(5)));
    validate(&tree).expect("the normal form");
    // A single pane becomes half of a split.
    let mut one = p(1);
    insert_at_root(&mut one, pane(2), SplitDirection::Up, None).expect("an insert");
    assert_tree(&one, &column(&[0.5, 0.5], vec![p(2), p(1)]));
    assert_eq!(
        insert_at_root(&mut one, pane(1), SplitDirection::Up, None),
        Err(TreeError::DuplicatePane)
    );
}

#[test]
fn the_validator_finds_each_way_to_break_the_normal_form() {
    assert!(validate(&p(1)).is_ok());
    for (what, tree) in [
        ("one child", row(&[1.0], vec![p(1)])),
        ("ratio count", row(&[1.0], vec![p(1), p(2)])),
        ("zero ratio", row(&[0.0, 1.0], vec![p(1), p(2)])),
        ("sum", row(&[0.4, 0.4], vec![p(1), p(2)])),
        ("nan", row(&[f32::NAN, 0.5], vec![p(1), p(2)])),
        (
            "same axis",
            row(&[0.5, 0.5], vec![p(1), row(&[0.5, 0.5], vec![p(2), p(3)])]),
        ),
        ("twice", row(&[0.5, 0.5], vec![p(1), p(1)])),
    ] {
        assert!(validate(&tree).is_err(), "{what}");
    }
}

// --- properties -------------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Op {
    Split {
        target: usize,
        direction: u8,
        ratio: Option<f32>,
    },
    InsertAtRoot {
        direction: u8,
        ratio: Option<f32>,
    },
    Remove {
        index: usize,
    },
    Equalize {
        path: Vec<usize>,
    },
    SetRatios {
        path: Vec<usize>,
        raw: Vec<f32>,
    },
}

fn direction(n: u8) -> SplitDirection {
    [
        SplitDirection::Left,
        SplitDirection::Right,
        SplitDirection::Up,
        SplitDirection::Down,
    ][usize::from(n % 4)]
}

fn op() -> impl Strategy<Value = Op> {
    let ratio = prop::option::of(0.02f32..0.98);
    prop_oneof![
        4 => (any::<usize>(), any::<u8>(), ratio.clone())
            .prop_map(|(target, direction, ratio)| Op::Split { target, direction, ratio }),
        1 => (any::<u8>(), ratio).prop_map(|(direction, ratio)| Op::InsertAtRoot { direction, ratio }),
        3 => any::<usize>().prop_map(|index| Op::Remove { index }),
        1 => prop::collection::vec(any::<usize>(), 0..4).prop_map(|path| Op::Equalize { path }),
        2 => (prop::collection::vec(any::<usize>(), 0..4), prop::collection::vec(0.05f32..1.0, 2..6))
            .prop_map(|(path, raw)| Op::SetRatios { path, raw }),
    ]
}

/// A path of child indices that leads to a split, from numbers that stand for "some child".
fn split_path(root: &LayoutNode, choices: &[usize]) -> Vec<u32> {
    let mut path = Vec::new();
    let mut node = root;
    for choice in choices {
        match node {
            LayoutNode::Split { children, .. } => {
                let at = choice % children.len();
                path.push(u32::try_from(at).expect("a small index"));
                node = &children[at];
            }
            LayoutNode::Pane { .. } => break,
        }
    }
    path
}

fn ratios_at<'a>(root: &'a LayoutNode, path: &[u32]) -> Option<&'a [f32]> {
    let mut node = root;
    for index in path {
        match node {
            LayoutNode::Split { children, .. } => node = children.get(*index as usize)?,
            LayoutNode::Pane { .. } => return None,
        }
    }
    match node {
        LayoutNode::Split { ratios, .. } => Some(ratios),
        LayoutNode::Pane { .. } => None,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    /// After any sequence of operations the tree is in normal form and has exactly the panes that
    /// were added and not removed, each once.
    #[test]
    fn any_operations_keep_the_normal_form_and_the_set_of_panes(ops in prop::collection::vec(op(), 1..60)) {
        let mut tree = p(0);
        let mut present = vec![pane(0)];
        let mut next = 1u128;
        for op in ops {
            match op {
                Op::Split { target, direction: d, ratio } => {
                    let target = present[target % present.len()];
                    let new = pane(next);
                    next += 1;
                    split(&mut tree, target, new, direction(d), ratio).expect("a split");
                    present.push(new);
                }
                Op::InsertAtRoot { direction: d, ratio } => {
                    let new = pane(next);
                    next += 1;
                    insert_at_root(&mut tree, new, direction(d), ratio).expect("an insert");
                    present.push(new);
                }
                Op::Remove { index } => {
                    if present.len() > 1 {
                        let gone = present.remove(index % present.len());
                        tree = remove(tree, gone).expect("a removal").expect("a tree is left");
                    }
                }
                Op::Equalize { path } => {
                    let path = split_path(&tree, &path);
                    equalize(&mut tree, &path).expect("an equalize");
                    if let Some(ratios) = ratios_at(&tree, &path) {
                        let first = ratios[0];
                        prop_assert!(ratios.iter().all(|r| (r - first).abs() < 1e-6), "{ratios:?}");
                    }
                }
                Op::SetRatios { path, raw } => {
                    let path = split_path(&tree, &path);
                    if let Some(current) = ratios_at(&tree, &path) {
                        // Ratios for exactly the children, from raw numbers, scaled to 1.
                        let total: f32 = raw.iter().take(current.len()).sum();
                        let wanted: Vec<f32> = (0..current.len())
                            .map(|i| raw.get(i).copied().unwrap_or(0.5) / total.max(0.1))
                            .collect();
                        let sum: f32 = wanted.iter().sum();
                        let scaled: Vec<f32> = wanted.iter().map(|r| r / sum).collect();
                        if scaled.iter().all(|r| *r >= MIN_RATIO) {
                            set_ratios(&mut tree, &path, &scaled).expect("ratios");
                            let now = ratios_at(&tree, &path).expect("the same split");
                            prop_assert!(now.iter().zip(&scaled).all(|(a, b)| (a - b).abs() < 1e-5));
                        }
                    }
                }
            }
            prop_assert_eq!(validate(&tree), Ok(()));
            let mut found = panes(&tree);
            found.sort();
            let mut expected = present.clone();
            expected.sort();
            prop_assert_eq!(found, expected);
        }
    }

    /// Splitting a pane and then closing the new pane keeps every other pane, in order.
    #[test]
    fn closing_the_pane_that_a_split_made_keeps_the_others_in_order(
        ops in prop::collection::vec(op(), 0..30),
        target in any::<usize>(),
        d in any::<u8>(),
    ) {
        let mut tree = p(0);
        let mut present = vec![pane(0)];
        let mut next = 1u128;
        for op in ops {
            if let Op::Split { target, direction: dir, ratio } = op {
                let at = present[target % present.len()];
                split(&mut tree, at, pane(next), direction(dir), ratio).expect("a split");
                present.push(pane(next));
                next += 1;
            }
        }
        let before = panes(&tree);
        let at = present[target % present.len()];
        let new = pane(next);
        split(&mut tree, at, new, direction(d), None).expect("a split");
        let tree = remove(tree, new).expect("a removal").expect("a tree");
        prop_assert_eq!(validate(&tree), Ok(()));
        prop_assert_eq!(panes(&tree), before);
    }
}
