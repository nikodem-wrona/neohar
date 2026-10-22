use super::*;
use crate::ErrorCode;
use crate::layout::{ops::NewEvent, testing::Counters};

#[test]
fn an_empty_layout_is_valid_and_has_nothing() {
    let layout = Layout::default();
    layout.validate().expect("valid");
    assert!(layout.workspaces.is_empty() && layout.panes.is_empty() && layout.groups.is_empty());
}

#[test]
fn the_validator_finds_a_layout_that_breaks_a_rule() {
    let mut alloc = Counters::new();
    let mut layout = Layout::default();
    layout
        .create_workspace(
            nh_proto::api::calls::workspace::WorkspaceCreate {
                name: Some("A".into()),
                root: None,
                group: None,
                machine: None,
                command: None,
            },
            &mut alloc,
        )
        .expect("created");
    layout.validate().expect("valid");

    let mut broken = layout.clone();
    broken.workspaces[0].selected_tab = None;
    assert!(broken.validate().is_err(), "tabs and no selection");

    let mut broken = layout.clone();
    let pane = *broken.panes.keys().next().expect("a pane");
    broken.panes.get_mut(&pane).expect("the pane").tab = TabId::new(ulid::Ulid::from(77u128));
    assert!(broken.validate().is_err(), "a pane of an unknown tab");

    let mut broken = layout.clone();
    broken.workspaces[0].tabs[0].zoomed = Some(PaneId::new(ulid::Ulid::from(78u128)));
    assert!(
        broken.validate().is_err(),
        "a zoom of a pane that is not there"
    );

    let mut broken = layout.clone();
    broken.workspaces[0].group = Some(GroupId::new(ulid::Ulid::from(79u128)));
    assert!(broken.validate().is_err(), "an unknown group");

    let mut broken = layout.clone();
    broken.panes.clear();
    assert!(
        broken.validate().is_err(),
        "a tab that shows a pane that is gone"
    );
}

#[test]
fn runtime_states_come_from_the_text_of_the_store() {
    assert_eq!(Runtime::parse("created", None), Runtime::Created);
    assert_eq!(Runtime::parse("running", None), Runtime::Running);
    assert_eq!(Runtime::parse("exited", Some(3)), Runtime::Exited(Some(3)));
    assert_eq!(Runtime::parse("lost", None), Runtime::Lost);
    assert!(Runtime::Running.is_live());
    assert!(
        ![Runtime::Created, Runtime::Exited(None), Runtime::Lost]
            .iter()
            .any(|r| r.is_live())
    );
}

#[test]
fn errors_map_to_the_codes_of_the_contract() {
    let code = |e: LayoutError| RpcError::from(e).code;
    assert_eq!(code(LayoutError::NotFound("x".into())), ErrorCode::NotFound);
    assert_eq!(
        code(LayoutError::Invalid("x".into())),
        ErrorCode::InvalidParams
    );
    assert_eq!(
        code(LayoutError::Unsupported("x".into())),
        ErrorCode::Unsupported
    );
    assert_eq!(code(LayoutError::Internal("x".into())), ErrorCode::Internal);
    let confirm = RpcError::from(LayoutError::Confirm {
        process: Some("claude".into()),
        is_agent: true,
    });
    assert_eq!(confirm.code, ErrorCode::ConfirmRequired);
    assert_eq!(
        confirm.details,
        serde_json::json!({"process": "claude", "is_agent": true})
    );
    assert_eq!(
        code(LayoutError::from(TreeError::NoSuchPane)),
        ErrorCode::NotFound
    );
    assert_eq!(
        code(LayoutError::from(TreeError::BadRatios)),
        ErrorCode::InvalidParams
    );
    let _ = std::marker::PhantomData::<NewEvent>;
}
