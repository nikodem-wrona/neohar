//! When a phone gets a push, with a sender that writes down its jobs.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use nh_agents::arbiter::{Input, Observation};
use nh_proto::api::{
    common::Source,
    ids::{RunId, WindowId},
    model::Execution,
    time::Timestamp,
};
use ulid::Ulid;

use crate::{
    agents::{
        runs,
        tests::{Fixture, Options},
    },
    focus::Screen,
    interaction::tests::{gate, pre_tool, started_with},
    persist::now_ms,
    phones::{PushJob, PushOut, PushOutcome, PushSubscription, PushUrgency},
};

/// The coalesce window of a test: short, and long enough for two rows inside it.
const WINDOW: Duration = Duration::from_millis(600);

/// How long a test waits before it says that no push went.
const QUIET: Duration = Duration::from_millis(250);

/// A sender that writes down its jobs and answers with the outcomes of the test, then
/// `Delivered`.
#[derive(Default)]
struct Recorder {
    jobs: Mutex<Vec<PushJob>>,
    outcomes: Mutex<VecDeque<PushOutcome>>,
}

impl Recorder {
    fn jobs(&self) -> Vec<PushJob> {
        self.jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn answer(&self, outcome: PushOutcome) {
        self.outcomes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(outcome);
    }
}

#[async_trait::async_trait]
impl PushOut for Recorder {
    fn check(&self, _: &PushSubscription) -> Result<(), String> {
        Ok(())
    }

    async fn send(&self, job: PushJob) -> PushOutcome {
        self.jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(job);
        self.outcomes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
            .unwrap_or(PushOutcome::Delivered)
    }
}

struct Rig {
    f: Fixture,
    run: RunId,
    push: Arc<Recorder>,
    phone: Ulid,
}

fn subscription(phone: Ulid) -> PushSubscription {
    PushSubscription {
        endpoint: format!("https://web.push.apple.com/{phone}"),
        p256dh: "key".to_owned(),
        auth: "auth".to_owned(),
    }
}

/// A core with a run, a paired phone with a subscription, and the recorder.
async fn rig(options: Options) -> Rig {
    let push = Arc::new(Recorder::default());
    let (f, run) = started_with(Fixture::with_options(Options {
        notify_window: Some(WINDOW),
        turn_settle: Some(Duration::from_millis(200)),
        push: Some(Arc::clone(&push) as Arc<dyn PushOut>),
        ..options
    }))
    .await;
    let phone = Ulid::from_parts(now_ms() as u64, 7);
    f.update(move |parts| {
        parts
            .state
            .phones
            .add_for_test(phone, Some(subscription(phone)));
    })
    .await;
    Rig {
        f,
        run,
        push,
        phone,
    }
}

impl Rig {
    async fn set(&self, execution: Execution) {
        let run = self.run;
        self.f.update(move |parts| set(parts, run, execution)).await;
    }

    /// A window that has the focus of the OS, or not, with the focus in the chrome: no pane of it
    /// has the focus.
    async fn desk(&self, window: WindowId, focused: bool) {
        self.f
            .thread
            .handle()
            .update(move |parts| {
                let facts = crate::layout::model::WindowFacts {
                    focused,
                    visible: true,
                    ..Default::default()
                };
                crate::actor::layout::apply_window_report(parts, window, facts);
            })
            .await
            .expect("the actor answers");
    }

    async fn screen(&self, locked: bool) {
        self.f
            .update(move |parts| {
                crate::focus::screen(
                    parts,
                    Screen {
                        locked,
                        display_asleep: false,
                    },
                );
            })
            .await;
    }

    async fn pushes(&self, count: usize) -> Vec<PushJob> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let jobs = self.push.jobs();
            if jobs.len() >= count {
                // A push too many would come right after.
                tokio::time::sleep(QUIET).await;
                let jobs = self.push.jobs();
                assert_eq!(jobs.len(), count, "pushes: {jobs:?}");
                return jobs;
            }
            assert!(
                Instant::now() < deadline,
                "{} of {count} pushes",
                jobs.len()
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

fn set(parts: &mut crate::actor::Parts<'_>, run: RunId, execution: Execution) {
    runs::input(
        parts,
        run,
        Input::new(
            Source::Structured,
            Timestamp(now_ms()),
            Observation::Execution(execution),
        ),
    );
}

#[tokio::test]
async fn with_no_window_a_new_row_gives_one_push_with_the_reason_only() {
    let r = rig(Options::default()).await;
    r.set(Execution::Error).await;
    let jobs = r.pushes(1).await;
    let job = &jobs[0];
    assert_eq!(job.device, r.phone);
    assert!(
        job.message.title.starts_with("Claude Code"),
        "{}",
        job.message.title
    );
    assert_eq!(job.message.body, "Stopped with an error");
    assert_eq!(job.message.url, format!("#/run/{}", r.run));
    assert_eq!(job.message.badge, Some(1));
    assert_eq!(job.urgency, PushUrgency::Normal);
    assert_eq!(job.topic.as_deref(), Some(r.run.to_string().as_str()));
}

#[tokio::test]
async fn a_focused_desktop_window_gives_no_push() {
    let r = rig(Options::default()).await;
    let window = r.f.open_window().await;
    r.desk(window, true).await;
    r.set(Execution::Error).await;
    r.pushes(0).await;
}

#[tokio::test]
async fn a_window_without_the_focus_counts_as_none() {
    let r = rig(Options::default()).await;
    let window = r.f.open_window().await;
    r.desk(window, false).await;
    r.set(Execution::Error).await;
    r.pushes(1).await;
}

#[tokio::test]
async fn a_row_that_came_with_the_focus_gets_no_push_when_the_window_loses_it() {
    let r = rig(Options::default()).await;
    let window = r.f.open_window().await;
    r.desk(window, true).await;
    r.set(Execution::Error).await;
    r.desk(window, false).await;
    r.pushes(0).await;
    // A row that comes now gets its push: a short switch to another app lets it go.
    r.set(Execution::Working).await;
    tokio::time::sleep(WINDOW).await;
    r.set(Execution::Error).await;
    r.pushes(1).await;
}

#[tokio::test]
async fn a_lock_pushes_each_open_row_without_a_push_and_then_new_rows() {
    let r = rig(Options::default()).await;
    let window = r.f.open_window().await;
    r.desk(window, true).await;
    r.set(Execution::Error).await;
    r.pushes(0).await;
    r.screen(true).await;
    let jobs = r.pushes(1).await;
    assert_eq!(jobs[0].message.body, "Stopped with an error");
    // A second lock report pushes nothing again.
    r.screen(true).await;
    r.pushes(1).await;
    // The window keeps the focus of the OS while the screen is locked: a new row still pushes.
    r.set(Execution::Working).await;
    tokio::time::sleep(WINDOW).await;
    r.set(Execution::Error).await;
    r.pushes(2).await;
    // The person is back.
    r.screen(false).await;
    r.set(Execution::Working).await;
    tokio::time::sleep(WINDOW).await;
    r.set(Execution::Error).await;
    r.pushes(2).await;
}

#[tokio::test]
async fn a_row_that_ends_before_its_push_gives_none() {
    let r = rig(Options::default()).await;
    let run = r.run;
    r.f.update(move |parts| {
        set(parts, run, Execution::Error);
        set(parts, run, Execution::Working);
    })
    .await;
    r.pushes(0).await;
}

#[tokio::test]
async fn a_visible_phone_gets_no_push() {
    let r = rig(Options::default()).await;
    let phone = r.phone;
    r.f.update(move |parts| parts.state.phones.set_visible(phone, 7, true))
        .await;
    r.set(Execution::Error).await;
    r.pushes(0).await;
    // The connection of the app ended: the phone is no longer visible.
    r.f.update(move |parts| parts.state.phones.set_visible(phone, 7, false))
        .await;
    r.set(Execution::Working).await;
    tokio::time::sleep(WINDOW).await;
    r.set(Execution::Error).await;
    r.pushes(1).await;
}

#[tokio::test]
async fn a_second_row_of_the_run_inside_the_coalesce_window_gives_one_push() {
    let r = rig(Options::default()).await;
    r.set(Execution::Error).await;
    r.pushes(1).await;
    r.set(Execution::Working).await;
    r.set(Execution::Error).await;
    r.pushes(1).await;
    // After the window, the next row pushes again.
    r.set(Execution::Working).await;
    tokio::time::sleep(WINDOW).await;
    r.set(Execution::Error).await;
    r.pushes(2).await;
}

#[tokio::test]
async fn a_question_has_its_short_text_and_no_text_of_the_command() {
    let r = rig(Options::default()).await;
    let command = "rm -rf /secret/project";
    pre_tool(&r.f, "c1", command).await;
    let (_start, rx) = gate(&r.f, 7, command).await;
    // The shim stays connected for the whole test.
    std::mem::forget(rx);
    let jobs = r.pushes(1).await;
    let job = &jobs[0];
    assert_eq!(job.message.body, "Waits for an approval");
    assert_eq!(job.urgency, PushUrgency::High);
    let text = serde_json::to_string(&job.message).expect("json");
    assert!(!text.contains("secret"), "no text of the command: {text}");
}

#[tokio::test]
async fn a_turn_end_pushes_finished_its_turn() {
    let r = rig(Options::default()).await;
    r.set(Execution::Working).await;
    r.set(Execution::Idle).await;
    let jobs = r.pushes(1).await;
    assert_eq!(jobs[0].message.body, "Finished its turn");
}

#[tokio::test]
async fn a_push_is_an_event_and_a_gone_subscription_ends() {
    let mut r = rig(Options::default()).await;
    r.set(Execution::Error).await;
    r.pushes(1).await;
    let deadline = Instant::now() + Duration::from_secs(5);
    while !r.f.kinds().iter().any(|kind| kind == "phone.pushed") {
        assert!(Instant::now() < deadline, "no phone.pushed");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let since =
        r.f.update(|parts| parts.state.native.push.since_delivered())
            .await;
    assert!(since.is_some(), "the keep awake knows the push");
    // The push service no longer knows the phone.
    r.push.answer(PushOutcome::Gone { status: 410 });
    r.set(Execution::Working).await;
    tokio::time::sleep(WINDOW).await;
    r.set(Execution::Error).await;
    r.pushes(2).await;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let targets =
            r.f.update(|parts| parts.state.phones.push_targets().len())
                .await;
        if targets == 0 {
            break;
        }
        assert!(Instant::now() < deadline, "the subscription stays");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(r.f.kinds().iter().any(|kind| kind == "phone.push_failed"));
}

#[tokio::test]
async fn the_rows_of_the_last_core_get_no_push() {
    let r = rig(Options::default()).await;
    let run = r.run;
    r.f.update(move |parts| {
        // The row is there before the rule looks: the last core left it.
        let slot = parts.state.native.push.slot.take();
        set(parts, run, Execution::Error);
        parts.state.native.push.slot = slot;
        // What a core does at its start, before the first publication of the runs.
        crate::native::push::adopt(parts.state);
    })
    .await;
    r.pushes(0).await;
    r.screen(true).await;
    r.pushes(0).await;
}

#[tokio::test]
async fn the_focus_of_the_desk_counts_after_its_debounce() {
    let debounce = Duration::from_millis(300);
    let r = rig(Options {
        focus_debounce: Some(debounce),
        ..Options::default()
    })
    .await;
    let window = r.f.open_window().await;
    r.desk(window, true).await;
    tokio::time::sleep(debounce * 2).await;
    // The window lost the focus a moment ago: the desk still counts as focused.
    r.desk(window, false).await;
    r.set(Execution::Error).await;
    r.pushes(0).await;
    r.set(Execution::Working).await;
    tokio::time::sleep(debounce * 2).await;
    r.set(Execution::Error).await;
    r.pushes(1).await;
}

/// A notice of another machine, as the host of the app gets it over the link.
fn machine_notice(interaction: bool, pane: bool) -> crate::host::HostNotification {
    use nh_proto::api::ids::{InteractionId, PaneId};
    crate::host::HostNotification {
        title: "Claude Code asks for permission".to_owned(),
        body: "Run rm -rf build".to_owned(),
        urgency: nh_proto::api::common::Urgency::High,
        interaction: interaction.then(|| InteractionId::new(Ulid::from_parts(1, 1))),
        pane: pane.then(|| PaneId::new(Ulid::from_parts(1, 2))),
        actions: Vec::new(),
    }
}

async fn notice_of(r: &Rig, machine: &str, notice: crate::host::HostNotification) {
    let machine = machine.to_owned();
    r.f.update(move |parts| super::machine_notice(parts, &machine, &notice))
        .await;
}

#[tokio::test]
async fn a_notice_of_a_machine_pushes_its_label_and_title_and_never_its_body() {
    let r = rig(Options::default()).await;
    notice_of(&r, "studio", machine_notice(true, false)).await;
    let jobs = r.pushes(1).await;
    let job = &jobs[0];
    assert_eq!(job.device, r.phone);
    assert_eq!(
        job.message.title,
        "studio · Claude Code asks for permission"
    );
    assert_eq!(job.message.body, "Waits for you");
    assert_eq!(job.message.url, "#/machine/studio");
    assert_eq!(job.message.badge, None);
    assert_eq!(job.urgency, PushUrgency::High);
    assert!(!format!("{job:?}").contains("rm -rf"));
    // The same notice again inside the coalesce window: no second push.
    notice_of(&r, "studio", machine_notice(true, false)).await;
    // A notice of a program (no interaction, no pane) has free text: no push.
    notice_of(&r, "studio", machine_notice(false, false)).await;
    r.pushes(1).await;
    // The turn end of a pane is a push too.
    notice_of(&r, "studio", machine_notice(false, true)).await;
    let jobs = r.pushes(2).await;
    assert_eq!(jobs[1].message.body, "Finished its turn");
    assert_eq!(jobs[1].urgency, PushUrgency::Normal);
}

#[tokio::test]
async fn a_notice_of_a_machine_gives_no_push_to_a_desk_with_the_focus_or_a_visible_phone() {
    let debounce = Duration::from_millis(200);
    let r = rig(Options {
        focus_debounce: Some(debounce),
        ..Options::default()
    })
    .await;
    let window = r.f.open_window().await;
    r.desk(window, true).await;
    tokio::time::sleep(debounce * 2).await;
    notice_of(&r, "studio", machine_notice(true, false)).await;
    r.pushes(0).await;
    r.desk(window, false).await;
    tokio::time::sleep(debounce * 2).await;
    let phone = r.phone;
    r.f.update(move |parts| parts.state.phones.set_visible(phone, 7, true))
        .await;
    notice_of(&r, "studio", machine_notice(true, false)).await;
    r.pushes(0).await;
    r.f.update(move |parts| parts.state.phones.set_visible(phone, 7, false))
        .await;
    notice_of(&r, "build-1", machine_notice(true, false)).await;
    r.pushes(1).await;
}
