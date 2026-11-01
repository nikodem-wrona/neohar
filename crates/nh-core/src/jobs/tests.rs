use std::time::Duration;

use serde_json::json;

use super::*;

fn registry() -> (Arc<Jobs>, Arc<ProjectionHub>) {
    let hub = ProjectionHub::new();
    (Jobs::new(Arc::clone(&hub)), hub)
}

/// The row of a job in the projection, as the last flush left it.
fn row(hub: &ProjectionHub, id: JobId) -> Option<Value> {
    hub.flush();
    hub.snapshot(Projection::Jobs)
        .items
        .get(&id.to_string())
        .cloned()
}

#[test]
fn a_new_job_runs_and_the_projection_shows_it() {
    let (jobs, hub) = registry();
    let job = jobs.start("Create the task").expect("a job");
    let shown = row(&hub, job.id()).expect("a row");
    assert_eq!(shown["id"], job.id().to_string());
    assert_eq!(shown["title"], "Create the task");
    assert_eq!(shown["status"], "running");
    assert!(shown.get("progress").is_none() && shown.get("finished_at").is_none());
    assert!(
        shown["started_at"]
            .as_str()
            .is_some_and(|t| t.ends_with('Z'))
    );
    assert_eq!(hub.snapshot(Projection::Jobs).order, [job.id().to_string()]);
}

#[test]
fn progress_is_kept_between_zero_and_one_and_a_message_can_follow() {
    let (jobs, hub) = registry();
    let job = jobs.start("Index").expect("a job");
    job.progress(0.25, "Reading");
    let shown = row(&hub, job.id()).expect("a row");
    assert_eq!(shown["progress"], 0.25);
    assert_eq!(shown["message"], "Reading");
    job.progress(7.0, "Over");
    assert_eq!(row(&hub, job.id()).expect("a row")["progress"], 1.0);
    job.progress(f32::NAN, "Unknown");
    assert_eq!(row(&hub, job.id()).expect("a row")["progress"], 0.0);
    job.message("Writing");
    let shown = row(&hub, job.id()).expect("a row");
    assert_eq!(shown["message"], "Writing");
    assert_eq!(shown["progress"], 0.0, "a message keeps the progress");
}

#[test]
fn a_job_that_is_done_has_its_result_and_does_not_change_again() {
    let (jobs, hub) = registry();
    let job = jobs.start("Create the task").expect("a job");
    let id = job.id();
    job.progress(0.5, "Half");
    job.done(json!({"task": "T1"}));
    let shown = row(&hub, id).expect("a row");
    assert_eq!(shown["status"], "done");
    assert_eq!(shown["progress"], 1.0);
    assert_eq!(shown["result"], json!({"task": "T1"}));
    assert!(shown["finished_at"].is_string());
    // Nothing moves a job that ended.
    jobs.update(id, |job| job.message = Some("late".to_owned()));
    assert_eq!(row(&hub, id).expect("a row")["message"], "Half");
}

#[test]
fn a_job_that_failed_has_its_error() {
    let (jobs, hub) = registry();
    let job = jobs.start("Create the task").expect("a job");
    let id = job.id();
    job.fail(CallError {
        code: ErrorCode::NotFound,
        message: "no such repository".to_owned(),
        details: Value::Null,
    });
    let shown = row(&hub, id).expect("a row");
    assert_eq!(shown["status"], "failed");
    assert_eq!(shown["error"]["code"], "not_found");
    assert_eq!(shown["error"]["message"], "no such repository");
}

#[test]
fn a_worker_that_drops_its_handle_fails_the_job() {
    let (jobs, hub) = registry();
    let job = jobs.start("Lost").expect("a job");
    let id = job.id();
    drop(job);
    let shown = row(&hub, id).expect("a row");
    assert_eq!(shown["status"], "failed");
    assert_eq!(shown["error"]["code"], "internal");

    // A worker that panics drops its handle on the way out.
    let job = jobs.start("Panics").expect("a job");
    let id = job.id();
    let result = std::thread::spawn(move || {
        let _job = job;
        panic!("a bug in a worker");
    })
    .join();
    assert!(result.is_err());
    assert_eq!(row(&hub, id).expect("a row")["status"], "failed");
}

#[tokio::test]
async fn cancel_marks_the_job_at_once_and_wakes_the_worker() {
    let (jobs, hub) = registry();
    let job = jobs.start("Fetch").expect("a job");
    let id = job.id();
    let token = job.token();
    assert!(!token.is_cancelled() && !job.is_cancelled());
    // The token stays pending while nobody cancels.
    assert!(
        tokio::time::timeout(Duration::from_millis(50), token.cancelled())
            .await
            .is_err()
    );
    let waiting = tokio::spawn({
        let token = token.clone();
        async move { token.cancelled().await }
    });

    jobs.cancel(id).expect("a cancel");
    tokio::time::timeout(Duration::from_secs(5), waiting)
        .await
        .expect("the worker wakes")
        .expect("no panic");
    assert!(token.is_cancelled() && job.is_cancelled());
    let shown = row(&hub, id).expect("a row");
    assert_eq!(shown["status"], "cancelled");
    assert!(shown["finished_at"].is_string());

    // What the worker reports after the cancel is ignored.
    job.progress(0.9, "Still going");
    job.done(json!({"hits": 3}));
    let shown = row(&hub, id).expect("a row");
    assert_eq!(shown["status"], "cancelled");
    assert!(shown.get("result").is_none());
}

#[test]
fn cancel_refuses_a_job_that_ended_and_a_job_that_does_not_exist() {
    let (jobs, _hub) = registry();
    let unknown = jobs.cancel(JobId::new(new_ulid().expect("an id")));
    assert_eq!(unknown.expect_err("no such job").code, ErrorCode::NotFound);

    let job = jobs.start("Done").expect("a job");
    let id = job.id();
    job.done(json!({}));
    let error = jobs.cancel(id).expect_err("the job ended");
    assert_eq!(error.code, ErrorCode::InvalidParams);
    assert!(error.message.contains("done"), "{error:?}");

    let job = jobs.start("Twice").expect("a job");
    let id = job.id();
    jobs.cancel(id).expect("the first cancel");
    assert_eq!(
        jobs.cancel(id).expect_err("the second").code,
        ErrorCode::InvalidParams
    );
    drop(job);
}

#[test]
fn only_the_last_jobs_that_ended_stay_and_a_running_job_always_stays() {
    let (jobs, hub) = registry();
    let running = jobs.start("Long").expect("a job");
    let mut ids = Vec::new();
    for n in 0..FINISHED_KEPT + 5 {
        let job = jobs.start(format!("Job {n}")).expect("a job");
        ids.push(job.id());
        job.done(json!({"n": n}));
    }
    hub.flush();
    let shown = hub.snapshot(Projection::Jobs);
    assert_eq!(shown.items.len(), FINISHED_KEPT + 1);
    for old in &ids[..5] {
        assert!(!shown.items.contains_key(&old.to_string()), "{old}");
        assert!(jobs.get(*old).is_none());
    }
    for kept in &ids[5..] {
        assert!(shown.items.contains_key(&kept.to_string()), "{kept}");
    }
    assert!(shown.items.contains_key(&running.id().to_string()));
    assert_eq!(shown.order.len(), FINISHED_KEPT + 1);
    // A cancel counts as an end too, and the oldest row makes room for it.
    jobs.cancel(running.id()).expect("a cancel");
    hub.flush();
    assert_eq!(hub.snapshot(Projection::Jobs).items.len(), FINISHED_KEPT);
    assert!(jobs.get(ids[5]).is_none());
    assert!(jobs.get(running.id()).is_some());
}
