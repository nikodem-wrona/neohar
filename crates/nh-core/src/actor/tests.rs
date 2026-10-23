use std::time::Duration;

use nh_proto::api::{events::EventPayload, projections::Projection};
use nh_store::{Fault, Tx};
use serde_json::{Value, json};

use super::*;
use crate::{ErrorCode, jobs::Jobs};

const GROUP: &str = "01M4F1W7EM2GN3EV6NHXZJEGGA";

/// The actor with test handlers. `group.create` writes a group, an event and a projection row,
/// and changes the state. The label of the machine stands for any state.
struct Fixture {
    thread: ActorThread,
    hub: Arc<ProjectionHub>,
    events: broadcast::Receiver<Event>,
    health: Arc<Health>,
    jobs: Arc<Jobs>,
}

impl Fixture {
    fn new(handlers: HandlerTable) -> Self {
        let hub = ProjectionHub::new();
        let (events_tx, events) = broadcast::channel(64);
        let health = Health::new(Arc::clone(&hub));
        let jobs = Jobs::new(Arc::clone(&hub));
        let thread = spawn(ActorConfig {
            store: Store::open_in_memory().expect("a store"),
            state: State::new(
                "test",
                1_000,
                Arc::new(crate::config::ConfigStore::load(std::env::temp_dir().join(
                    format!("nh-actor-test-{}-{}.toml", std::process::id(), line!()),
                ))),
                Arc::clone(&jobs),
                Arc::clone(&health),
            ),
            handlers,
            projections: Arc::clone(&hub),
            events: events_tx,
            health: Arc::clone(&health),
        })
        .expect("the actor starts");
        Self {
            thread,
            hub,
            events,
            health,
            jobs,
        }
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let call = Call::parse(method, params).expect("a call");
        self.thread.handle().call(call, Origin::ui()).await
    }

    async fn groups(&self) -> i64 {
        self.thread
            .handle()
            .store(|store| {
                Ok(store
                    .tx(0, |tx| {
                        Ok(tx
                            .sql()
                            .query_row("SELECT count(*) FROM groups", [], |r| r.get::<_, i64>(0))?)
                    })?
                    .0)
            })
            .await
            .expect("a count")
    }

    fn drain(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            out.push(event);
        }
        out
    }
}

fn insert_group(tx: &Tx<'_>, id: &str) -> Result<(), RpcError> {
    tx.sql()
        .execute(
            "INSERT INTO groups (id, name, sort_key, created_at, updated_at) VALUES (?1, 'g', 'a', 0, 0)",
            [id],
        )
        .map(|_| ())
        .map_err(|e| RpcError::internal(e.to_string()))
}

fn append(tx: &mut Tx<'_>, id: &str) {
    let group = nh_proto::api::ids::GroupId::new(ulid::Ulid::from_parts(1, 1));
    let _ = id;
    tx.append(
        Some(id),
        &EventPayload::GroupCreated(nh_proto::api::events::GroupEvent {
            group,
            name: Some(id.to_owned()),
        }),
    )
    .expect("an event");
}

/// A handler that works: a row, an event, a projection row and a change of the state.
fn group_create(ctx: &Ctx<'_>, tx: &mut Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::GroupCreate(params) = call else {
        return Err(RpcError::internal("not group.create"));
    };
    let id = params.name.clone();
    insert_group(tx, &id)?;
    append(tx, &id);
    let now = ctx.now_ms;
    Ok(Handled {
        result: json!({"group": id}),
        mutation: Some(Box::new(move |state, hub| {
            state.machine_label = format!("after-{id}");
            hub.set(Projection::Notifications, &id, json!({"at": now}));
        })),
    })
}

/// A handler that writes a row and an event and then fails.
fn group_rename_fails(_: &Ctx<'_>, tx: &mut Tx<'_>, _: &Call) -> Result<Handled, RpcError> {
    insert_group(tx, "doomed")?;
    append(tx, "doomed");
    Err(RpcError::new(ErrorCode::NotFound, "no such group"))
}

/// A handler that writes a row and panics.
fn group_move_panics(_: &Ctx<'_>, tx: &mut Tx<'_>, _: &Call) -> Result<Handled, RpcError> {
    insert_group(tx, "panicked")?;
    panic!("a bug in a handler");
}

/// A read: it returns the label of the machine.
fn group_list(ctx: &Ctx<'_>, _: &mut Tx<'_>, _: &Call) -> Result<Handled, RpcError> {
    Ok(Handled::result(json!({"label": ctx.state.machine_label})))
}

fn test_handlers() -> HandlerTable {
    let mut table = HandlerTable::new();
    table.insert(Method::GroupCreate, group_create);
    table.insert(Method::GroupRename, group_rename_fails);
    table.insert(Method::GroupMove, group_move_panics);
    table.insert(Method::GroupList, group_list);
    session::handlers(&mut table);
    table
}

#[tokio::test]
async fn a_command_commits_its_rows_its_events_and_then_the_state_and_the_projections() {
    let mut f = Fixture::new(test_handlers());
    let result = f
        .call("group.create", json!({"name": "one"}))
        .await
        .expect("a result");
    assert_eq!(result, json!({"group": "one"}));
    assert_eq!(f.groups().await, 1);
    let events = f.drain();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "group.created");
    assert_eq!(events[0].subject.as_deref(), Some("one"));
    assert_eq!(
        f.call("group.list", json!({})).await.expect("a read"),
        json!({"label": "after-one"})
    );
    f.hub.flush();
    assert_eq!(f.hub.snapshot(Projection::Notifications).order, ["one"]);
}

#[tokio::test]
async fn a_failing_handler_leaves_the_rows_the_events_the_state_and_the_projections_as_they_were() {
    let mut f = Fixture::new(test_handlers());
    f.call("group.create", json!({"name": "kept"}))
        .await
        .expect("a result");
    f.drain();
    f.hub.flush();
    let version = f.hub.snapshot(Projection::Notifications).version;

    let error = f
        .call("group.rename", json!({"group": GROUP, "name": "y"}))
        .await
        .expect_err("the handler fails");
    assert_eq!(
        error.code,
        ErrorCode::NotFound,
        "the caller gets the error of the handler"
    );
    assert_eq!(f.groups().await, 1, "the row of the failed command is gone");
    assert!(f.drain().is_empty(), "so is its event");
    assert_eq!(f.hub.flush(), 0, "no projection changed");
    assert_eq!(f.hub.snapshot(Projection::Notifications).version, version);
    assert_eq!(
        f.call("group.list", json!({})).await.expect("a read"),
        json!({"label": "after-kept"}),
        "and the state"
    );
}

#[tokio::test]
async fn a_handler_that_panics_costs_nothing_and_the_actor_goes_on() {
    let mut f = Fixture::new(test_handlers());
    let error = f
        .call("group.move", json!({"group": GROUP}))
        .await
        .expect_err("the handler panics");
    assert_eq!(error.code, ErrorCode::Internal);
    assert!(
        error.message.contains("nothing was changed"),
        "{}",
        error.message
    );
    assert_eq!(f.groups().await, 0, "the transaction rolled back");
    assert!(f.drain().is_empty());
    // The next command works.
    f.call("group.create", json!({"name": "after"}))
        .await
        .expect("a result");
    assert_eq!(f.groups().await, 1);
}

#[tokio::test]
async fn a_method_without_a_handler_is_unsupported() {
    let f = Fixture::new(test_handlers());
    let call = Call::parse("tab.close", json!({"tab": "w1:t1"})).expect("a call");
    let error = f
        .thread
        .handle()
        .call(call, Origin::ui())
        .await
        .expect_err("no handler");
    assert_eq!(error.code, ErrorCode::Unsupported);
    assert!(f.thread.handle().handles("group.create"));
    assert!(!f.thread.handle().handles("tab.close"));
    assert!(!f.thread.handle().handles("no.such.method"));
}

#[tokio::test]
async fn commands_run_one_at_a_time_in_the_order_of_their_arrival() {
    let mut f = Fixture::new(test_handlers());
    let handle = f.thread.handle();
    let calls: Vec<_> = (0..50)
        .map(|n| {
            let handle = handle.clone();
            tokio::spawn(async move {
                let call =
                    Call::parse("group.create", json!({"name": format!("g{n}")})).expect("a call");
                handle.call(call, Origin::ui()).await
            })
        })
        .collect();
    for call in calls {
        call.await.expect("a task").expect("a result");
    }
    assert_eq!(f.groups().await, 50);
    let events = f.drain();
    assert_eq!(events.len(), 50);
    let seqs: Vec<u64> = events.iter().map(|e| e.seq).collect();
    assert_eq!(
        seqs,
        (1..=50).collect::<Vec<u64>>(),
        "the events are in the order of the commits"
    );
}

#[tokio::test]
async fn a_store_job_runs_on_the_actor_in_its_turn() {
    let f = Fixture::new(test_handlers());
    let handle = f.thread.handle();
    let create = {
        let handle = handle.clone();
        tokio::spawn(async move {
            handle
                .call(
                    Call::parse("group.create", json!({"name": "first"})).expect("a call"),
                    Origin::ui(),
                )
                .await
        })
    };
    create.await.expect("a task").expect("a result");
    let count = handle
        .store(|store| {
            Ok(store
                .tx(0, |tx| {
                    Ok(tx.sql().query_row(
                        "SELECT count(*) FROM groups WHERE id = 'first'",
                        [],
                        |r| r.get::<_, i64>(0),
                    )?)
                })?
                .0)
        })
        .await
        .expect("a job");
    assert_eq!(count, 1);
    // The blocking form works from a thread that is not the actor.
    let blocking = tokio::task::spawn_blocking({
        let handle = handle.clone();
        move || handle.store_blocking(|store| store.schema_version())
    })
    .await
    .expect("a thread")
    .expect("a job");
    assert_eq!(blocking, nh_store::SCHEMA_VERSION);
}

#[tokio::test]
async fn a_store_job_that_panics_is_an_error_for_its_caller_and_not_for_the_actor() {
    let f = Fixture::new(test_handlers());
    let handle = f.thread.handle();
    let result = handle.store::<()>(|_| panic!("a bug in a job")).await;
    assert!(result.is_err());
    assert!(f.call("group.list", json!({})).await.is_ok());
}

#[tokio::test]
async fn a_degraded_store_refuses_commands_that_persist_and_serves_the_others() {
    let f = Fixture::new(test_handlers());
    let handle = f.thread.handle();
    f.call("group.create", json!({"name": "before"}))
        .await
        .expect("a result");
    handle
        .store(|store| {
            store.inject_fault(Some(Fault::DiskFull));
            Ok(())
        })
        .await
        .expect("a job");

    let error = f
        .call("group.create", json!({"name": "during"}))
        .await
        .expect_err("the store is full");
    assert_eq!(error.code, ErrorCode::DegradedStorage);
    assert!(
        error.details["reason"]
            .as_str()
            .is_some_and(|r| r.contains("full")),
        "{error:?}"
    );
    // A command that needs no store goes on.
    assert!(f.call("group.list", json!({})).await.is_ok());
    assert!(f.call("session.info", json!({})).await.is_ok());
    // It stays degraded even when the fault is gone, until a probe works.
    handle
        .store(|store| {
            store.inject_fault(None);
            Ok(())
        })
        .await
        .expect("a job");
    let again = f.call("group.create", json!({"name": "still"})).await;
    assert!(
        matches!(&again, Err(e) if e.code == ErrorCode::DegradedStorage) || again.is_ok(),
        "the actor probes by itself, so either answer is right: {again:?}"
    );
    // A probe at a later time ends the mode for sure.
    handle
        .store(|store| {
            store.probe(i64::MAX / 2)?;
            Ok(())
        })
        .await
        .expect("a probe");
    f.call("group.create", json!({"name": "after"}))
        .await
        .expect("the store works again");
}

#[tokio::test]
async fn the_palette_disables_the_commands_that_write_while_the_store_is_degraded() {
    let f = Fixture::new(default_handlers());
    f.call("workspace.create", json!({"name": "A"}))
        .await
        .expect("a workspace");
    let split = |value: &Value| -> (bool, Option<String>) {
        let item = &value["items"][0]["command"];
        assert_eq!(item["id"], "pane.split_right", "{value}");
        (
            item["enabled"].as_bool().expect("enabled"),
            item["disabled_reason"].as_str().map(str::to_owned),
        )
    };
    let query = |text: &str| json!({ "query": text });
    let before = f.call("palette.query", query("split pane right")).await;
    assert_eq!(split(&before.expect("a result")), (true, None));

    f.thread
        .handle()
        .store(|store| {
            store.inject_fault(Some(Fault::DiskFull));
            Ok(())
        })
        .await
        .expect("a job");
    // The command that fails is how the actor learns of the fault. It tells `health` and the palette.
    let error = f
        .call("workspace.create", json!({"name": "B"}))
        .await
        .expect_err("the store is full");
    assert_eq!(error.code, ErrorCode::DegradedStorage);

    let during = f
        .call("palette.query", query("split pane right"))
        .await
        .expect("a read goes on");
    let (enabled, reason) = split(&during);
    assert!(!enabled);
    assert!(
        reason.is_some_and(|r| r.starts_with("Storage is degraded") && r.contains("full")),
        "{during}"
    );
    // A command of the page and the settings do not write, so they stay.
    let page = f
        .call("palette.query", query("search scrollback"))
        .await
        .expect("a result");
    assert_eq!(page["items"][0]["command"]["enabled"], true);
    let settings = f
        .call("palette.query", query("open settings"))
        .await
        .expect("a result");
    assert_eq!(settings["items"][0]["command"]["enabled"], true);
    // The projection has the same state.
    f.hub.flush();
    let shown = f.hub.snapshot(Projection::Palette);
    let commands = shown.items["_"]["commands"].as_array().expect("commands");
    let command = |id: &str| commands.iter().find(|c| c["id"] == id).expect("a command");
    assert_eq!(command("pane.split_right")["enabled"], false);
    assert_eq!(command("search.open")["enabled"], true);
    // The page renames with a method that writes, so a rename is disabled too.
    assert_eq!(command("tab.rename")["enabled"], false);
    assert_eq!(command("workspace.rename")["enabled"], false);

    // The store works again, and the palette follows.
    f.thread
        .handle()
        .store(|store| {
            store.inject_fault(None);
            store.probe(i64::MAX / 2)?;
            Ok(())
        })
        .await
        .expect("a probe");
    f.call("workspace.create", json!({"name": "C"}))
        .await
        .expect("the store works again");
    let after = f
        .call("palette.query", query("split pane right"))
        .await
        .expect("a result");
    assert_eq!(split(&after), (true, None));
}

#[tokio::test]
async fn session_info_and_machine_info_describe_the_core() {
    let f = Fixture::new(test_handlers());
    let session = f
        .call("session.info", json!({}))
        .await
        .expect("session.info");
    assert_eq!(session["name"], "test");
    assert_eq!(session["api_version"], "neohar/1");
    assert_eq!(session["pid"], std::process::id());
    assert_eq!(session["log_epoch"], 1);
    assert_eq!(session["session_uuid"].as_str().map(str::len), Some(26));
    assert_eq!(session["protocols"], json!({"frame": 1, "holder": 2}));
    assert_eq!(session["started_at"], "1970-01-01T00:00:01.000Z");
    let machine = f
        .call("machine.info", json!({}))
        .await
        .expect("machine.info");
    assert_eq!(machine["label"], "local");
    assert_eq!(machine["os"], std::env::consts::OS);
    assert_eq!(machine["uuid"], session["machine_uuid"]);
}

#[tokio::test]
async fn stop_ends_the_actor_after_the_commands_before_it() {
    let mut f = Fixture::new(test_handlers());
    let handle = f.thread.handle();
    f.call("group.create", json!({"name": "last"}))
        .await
        .expect("a result");
    f.thread.stop();
    let call = Call::parse("group.list", json!({})).expect("a call");
    let error = handle
        .call(call, Origin::ui())
        .await
        .expect_err("the actor ended");
    assert_eq!(error.code, ErrorCode::Internal);
    let late = tokio::time::timeout(Duration::from_secs(2), handle.store(|_| Ok(()))).await;
    assert!(
        matches!(late, Ok(Err(_))),
        "a job after the end is an error, not a hang"
    );
}

/// The actor with the handlers of the product and a configuration file in a new directory.
fn config_fixture(name: &str) -> (Fixture, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("nh-actor-config-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory");
    let path = dir.join("config.toml");
    let hub = ProjectionHub::new();
    let (events_tx, events) = broadcast::channel(64);
    let config = Arc::new(crate::config::ConfigStore::load(path.clone()));
    let health = Health::new(Arc::clone(&hub));
    crate::config::publish_all(&config, &hub, &health);
    let jobs = Jobs::new(Arc::clone(&hub));
    let thread = spawn(ActorConfig {
        store: Store::open_in_memory().expect("a store"),
        state: State::new(
            "test",
            1_000,
            config,
            Arc::clone(&jobs),
            Arc::clone(&health),
        ),
        handlers: default_handlers(),
        projections: Arc::clone(&hub),
        events: events_tx,
        health: Arc::clone(&health),
    })
    .expect("the actor starts");
    (
        Fixture {
            thread,
            hub,
            events,
            health,
            jobs,
        },
        path,
    )
}

#[tokio::test]
async fn config_get_set_and_reload_work_on_the_file_and_the_projection() {
    let (f, path) = config_fixture("methods");
    let all = f.call("config.get", json!({})).await.expect("a result");
    assert_eq!(
        all["sections"]["approvals"]["entries"]["mode"]["value"],
        "auto"
    );
    assert_eq!(
        all["sections"]["approvals"]["entries"]["mode"]["source"],
        "default"
    );
    let one = f
        .call("config.get", json!({"section": "events"}))
        .await
        .expect("a result");
    assert_eq!(
        one["sections"].as_object().map(serde_json::Map::len),
        Some(1)
    );
    let missing = f
        .call("config.get", json!({"section": "nothing"}))
        .await
        .expect_err("no such section");
    assert_eq!(missing.code, ErrorCode::NotFound);

    f.call(
        "config.set",
        json!({"key": "approvals.mode", "value": "gate"}),
    )
    .await
    .expect("a set");
    assert_eq!(
        std::fs::read_to_string(&path).expect("the file"),
        "[approvals]\nmode = \"gate\"\n"
    );
    f.hub.flush();
    let published = f.hub.snapshot(Projection::Settings);
    assert_eq!(
        published.items["approvals"]["entries"]["mode"]["value"],
        "gate"
    );
    assert_eq!(
        published.items["approvals"]["entries"]["mode"]["source"],
        "ui"
    );
    let after = f.call("config.get", json!({})).await.expect("a result");
    assert_eq!(
        after["sections"]["approvals"]["entries"]["mode"]["value"],
        "gate"
    );

    // A wrong value is a typed error, and nothing changes.
    let bad = f
        .call(
            "config.set",
            json!({"key": "approvals.mode", "value": "sometimes"}),
        )
        .await
        .expect_err("a bad value");
    assert_eq!(bad.code, ErrorCode::InvalidParams);
    let unknown = f
        .call("config.set", json!({"key": "approvals.mood", "value": 1}))
        .await
        .expect_err("an unknown key");
    assert_eq!(unknown.code, ErrorCode::InvalidParams);
    assert!(unknown.message.contains("approvals.mood"), "{unknown:?}");
    assert_eq!(
        std::fs::read_to_string(&path).expect("the file"),
        "[approvals]\nmode = \"gate\"\n"
    );

    // The user edits the file, and reload reads it. A bad file is reported, and the old value stays.
    std::fs::write(&path, "[approvals]\nmode = \"observe\"\n").expect("a file");
    let reloaded = f.call("config.reload", json!({})).await.expect("a result");
    assert_eq!(reloaded["errors"], json!([]));
    f.hub.flush();
    assert_eq!(
        f.hub.snapshot(Projection::Settings).items["approvals"]["entries"]["mode"],
        json!({"value": "observe", "source": "file"})
    );
    std::fs::write(&path, "[approvals\n").expect("a file");
    let reloaded = f.call("config.reload", json!({})).await.expect("a result");
    assert_eq!(reloaded["errors"].as_array().map(Vec::len), Some(1));
    let after = f
        .call("config.get", json!({"section": "approvals"}))
        .await
        .expect("a result");
    assert_eq!(
        after["sections"]["approvals"]["entries"]["mode"]["value"],
        "observe"
    );
    let _ = std::fs::remove_dir_all(path.parent().expect("a parent"));
}

#[tokio::test]
async fn the_configuration_works_while_the_store_is_degraded() {
    let (f, path) = config_fixture("degraded");
    f.thread
        .handle()
        .store(|store| {
            store.inject_fault(Some(Fault::DiskFull));
            Ok(())
        })
        .await
        .expect("a job");
    f.call("config.get", json!({})).await.expect("get works");
    f.call("config.set", json!({"key": "agents.shims", "value": false}))
        .await
        .expect("set works: the file is not the database");
    f.call("config.reload", json!({}))
        .await
        .expect("reload works");
    assert_eq!(
        std::fs::read_to_string(&path).expect("the file"),
        "[agents]\nshims = false\n"
    );
    let _ = std::fs::remove_dir_all(path.parent().expect("a parent"));
}

#[tokio::test]
async fn the_health_follows_the_store_when_it_degrades_and_when_it_recovers() {
    let f = Fixture::new(test_handlers());
    let handle = f.thread.handle();
    assert_eq!(f.health.value().storage, StorageHealth::Ok);
    handle
        .store(|store| {
            store.inject_fault(Some(Fault::DiskFull));
            Ok(())
        })
        .await
        .expect("a job");
    let error = f
        .call("group.create", json!({"name": "during"}))
        .await
        .expect_err("the store is full");
    assert_eq!(error.code, ErrorCode::DegradedStorage);
    let StorageHealth::Degraded { reason } = f.health.value().storage else {
        panic!("the health shows a working store");
    };
    assert!(reason.contains("full"), "{reason}");
    f.hub.flush();
    assert_eq!(
        f.hub.snapshot(Projection::Health).items["_"]["storage"]["state"],
        "degraded"
    );
    // The probe at a later time ends the mode, and the health follows at once.
    handle
        .store(|store| {
            store.inject_fault(None);
            store.probe(i64::MAX / 2)?;
            Ok(())
        })
        .await
        .expect("a probe");
    assert_eq!(f.health.value().storage, StorageHealth::Ok);
}

#[tokio::test]
async fn jobs_cancel_works_through_the_actor_also_when_the_store_is_degraded() {
    let f = Fixture::new(default_handlers());
    let job = f.jobs.start("Search").expect("a job");
    let token = job.token();
    f.thread
        .handle()
        .store(|store| {
            store.inject_fault(Some(Fault::DiskFull));
            Ok(())
        })
        .await
        .expect("a job");
    f.call("jobs.cancel", json!({"job": job.id().to_string()}))
        .await
        .expect("a cancel: the registry is not in the database");
    assert!(token.is_cancelled());
    f.hub.flush();
    assert_eq!(
        f.hub.snapshot(Projection::Jobs).items[&job.id().to_string()]["status"],
        "cancelled"
    );
    let again = f
        .call("jobs.cancel", json!({"job": job.id().to_string()}))
        .await
        .expect_err("it ended");
    assert_eq!(again.code, ErrorCode::InvalidParams);
    let unknown = f
        .call("jobs.cancel", json!({"job": "01ARZ3NDEKTSV4RRFFQ69G5FAV"}))
        .await
        .expect_err("no such job");
    assert_eq!(unknown.code, ErrorCode::NotFound);
}

#[tokio::test]
async fn a_bad_config_file_is_a_warning_in_the_health_until_a_good_one_comes() {
    let (f, path) = config_fixture("health");
    assert!(f.health.value().warnings.is_empty());
    std::fs::write(&path, "[approvals\n").expect("a file");
    let reloaded = f.call("config.reload", json!({})).await.expect("a result");
    assert_eq!(reloaded["errors"].as_array().map(Vec::len), Some(1));
    let warnings = f.health.value().warnings;
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].id, "config:0");
    assert!(warnings[0].text.contains("config.toml"), "{warnings:?}");
    f.hub.flush();
    assert_eq!(
        f.hub.snapshot(Projection::Health).items["_"]["warnings"][0]["id"],
        "config:0"
    );
    // A key that this version does not know is a note too.
    std::fs::write(&path, "[approvals]\nmood = 1\n").expect("a file");
    f.call("config.reload", json!({})).await.expect("a result");
    assert!(f.health.value().warnings[0].text.contains("approvals.mood"));
    std::fs::write(&path, "[approvals]\nmode = \"gate\"\n").expect("a file");
    f.call("config.reload", json!({})).await.expect("a result");
    assert!(f.health.value().warnings.is_empty());
    let _ = std::fs::remove_dir_all(path.parent().expect("a parent"));
}
