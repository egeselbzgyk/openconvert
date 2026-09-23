//! The queue: every dropped PDF is a job, and one runs at a time (Phase 12 detail 5).
//!
//! v1 runs `desktop.max_concurrent_jobs` conversions at once — one — because the app owns a single
//! `-np 1` `llama-server` and unbounded parallelism would start N engines each wanting it (RT A5).
//! Dropping forty PDFs therefore makes forty jobs, one running and thirty-nine waiting in the
//! order they were dropped, each able to be cancelled or removed (A12.1).
//!
//! The queue is plain state plus a [`Launch`]: no window, no clock of its own. The app calls
//! [`JobQueue::tick`] on a timer to notice exits and enforce the kill deadline; the tests call it
//! with whatever `now` they need, which is how the five-second fallback is tested without five
//! seconds of sleeping.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use oc_core::jobspec::{JobSpec, LimitsSpec};
use oc_core::thresholds::T;
use oc_model::document::PresetName;
use serde::Serialize;

use crate::engine::{Engine, Launch, Running, UiError};
use crate::fs_scope::{default_output_for, free_output_path_among};

/// A job's state as the queue knows it. What happens *inside* a run — stages, progress,
/// heartbeats — is the engine's events, which go straight to the UI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum JobState {
    /// Waiting; `position` counts the running job as #1 (design decision 6).
    Queued {
        position: usize,
    },
    Running,
    /// Cancel was sent; waiting for `done{cancelled}`, killed at `ipc.kill_after_secs`.
    Cancelling,
    /// Removed from the queue before it ever started.
    CancelledBeforeStart,
    /// The engine exited with this code (`None` when a signal ended it).
    Exited {
        code: Option<i32>,
    },
    /// It could not be started at all.
    FailedToStart {
        error: UiError,
    },
}

/// One row of the queue, as the UI renders it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct JobView {
    pub id: String,
    pub input: PathBuf,
    pub output: PathBuf,
    /// The output name differs from `<input>.epub` because that file already existed.
    pub renamed: bool,
    /// This run was given the password the user typed on the row; a password failure now means
    /// that password was wrong, not that none was tried.
    pub unlocked: bool,
    /// This run applies the user's corrections to a book the queue already converted ("Fix and
    /// rebuild"): it replaces that output, and resumes after `structure` when it can (A12.4b).
    pub rebuild: bool,
    #[serde(flatten)]
    pub state: JobState,
}

/// What the queue tells the app, which relays it to the webview.
pub trait QueueSink: Send + Sync {
    /// A line of NDJSON from job `job`'s engine.
    fn line(&self, job: &str, line: String);
    /// Job `job` changed state outside its event stream (started, exited, removed).
    fn changed(&self, job: &JobView);
}

struct Job {
    id: String,
    input: PathBuf,
    output: PathBuf,
    renamed: bool,
    preset: PresetName,
    limits: Option<LimitsSpec>,
    /// The password a user typed for this job: kept in memory until the engine starts, then
    /// dropped (design decision 13: "used for this job only, never saved").
    password: Option<String>,
    /// Started from [`JobQueue::unlock`]. Only the fact is kept, never the password.
    unlocked: bool,
    /// Set by [`JobQueue::rebuild`]: the corrections to apply.
    rebuild: Option<Rebuild>,
    phase: Phase,
}

/// A "Fix and rebuild": the corrections file, and the digest of the PDF they were made for — which
/// the job spec carries, so a PDF changed since is refused (`E_INPUT_CHANGED`) rather than
/// corrected with ids that name other headings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rebuild {
    pub overrides: PathBuf,
    pub sha256: String,
}

enum Phase {
    Queued,
    Running(Box<dyn Running>),
    Cancelling {
        running: Box<dyn Running>,
        asked: Instant,
        killed: bool,
    },
    Done(JobState),
}

/// The queue.
pub struct JobQueue<L: Launch> {
    engine: Engine<L>,
    sink: Arc<dyn QueueSink>,
    max_concurrent: usize,
    kill_after: Duration,
    jobs: VecDeque<Job>,
    next: u64,
    /// Every event line each job's engine wrote, for its diagnostic bundle.
    logs: Arc<std::sync::Mutex<std::collections::BTreeMap<String, Vec<String>>>>,
}

impl<L: Launch> JobQueue<L> {
    pub fn new(engine: Engine<L>, sink: Arc<dyn QueueSink>) -> Self {
        Self {
            engine,
            sink,
            max_concurrent: usize::try_from(T.desktop.max_concurrent_jobs)
                .unwrap_or(usize::MAX)
                .max(1),
            kill_after: Duration::from_secs(
                u64::try_from(T.ipc.kill_after_secs).unwrap_or(u64::MAX),
            ),
            jobs: VecDeque::new(),
            next: 0,
            logs: Arc::default(),
        }
    }

    /// Add one job per PDF, in order, and start what may start. Returns the new ids.
    ///
    /// Each output is `<input>.epub`, or the first free `name (n).epub` when that exists or is
    /// already another queued job's output — the app never overwrites (design, `result.html` §5).
    pub fn enqueue(&mut self, pdfs: &[PathBuf], preset: PresetName) -> Vec<String> {
        self.enqueue_with(pdfs, preset, None)
    }

    /// [`JobQueue::enqueue`], with the resource caps the user set in Settings › Advanced.
    pub fn enqueue_with(
        &mut self,
        pdfs: &[PathBuf],
        preset: PresetName,
        limits: Option<LimitsSpec>,
    ) -> Vec<String> {
        let mut ids = Vec::with_capacity(pdfs.len());
        for input in pdfs {
            self.next += 1;
            let id = format!("job-{}", self.next);
            let desired = default_output_for(input);
            let output = free_output_path_among(&desired, |path| self.reserves(path));
            self.jobs.push_back(Job {
                id: id.clone(),
                input: input.clone(),
                renamed: output != desired,
                output,
                preset,
                limits,
                password: None,
                unlocked: false,
                rebuild: None,
                phase: Phase::Queued,
            });
            ids.push(id);
        }
        self.pump();
        self.announce_all();
        ids
    }

    /// Convert job `id`'s PDF again with the password the user typed on its row. The old row is
    /// replaced by the new job, which keeps the password in memory only until its engine starts.
    pub fn unlock(&mut self, id: &str, password: String) -> Result<String, UiError> {
        let index = self
            .jobs
            .iter()
            .position(|job| job.id == id)
            .ok_or_else(|| UiError::UnknownJob(id.to_owned()))?;
        if !matches!(self.jobs[index].phase, Phase::Done(_)) {
            return Err(UiError::JobBusy(id.to_owned()));
        }
        let old = self
            .jobs
            .remove(index)
            .ok_or_else(|| UiError::UnknownJob(id.to_owned()))?;
        self.next += 1;
        let new_id = format!("job-{}", self.next);
        self.jobs.push_back(Job {
            id: new_id.clone(),
            input: old.input,
            output: old.output,
            renamed: old.renamed,
            preset: old.preset,
            limits: old.limits,
            password: Some(password),
            unlocked: true,
            // A locked book being rebuilt keeps its corrections through the unlock.
            rebuild: old.rebuild,
            phase: Phase::Queued,
        });
        self.pump();
        self.announce_all();
        Ok(new_id)
    }

    /// Convert job `id`'s book again with the user's corrections ("Fix and rebuild"), replacing
    /// its output. The row is replaced by the new job, as for [`JobQueue::unlock`].
    pub fn rebuild(&mut self, id: &str, rebuild: Rebuild) -> Result<String, UiError> {
        let index = self
            .jobs
            .iter()
            .position(|job| job.id == id)
            .ok_or_else(|| UiError::UnknownJob(id.to_owned()))?;
        if !matches!(self.jobs[index].phase, Phase::Done(_)) {
            return Err(UiError::JobBusy(id.to_owned()));
        }
        let old = self
            .jobs
            .remove(index)
            .ok_or_else(|| UiError::UnknownJob(id.to_owned()))?;
        self.next += 1;
        let new_id = format!("job-{}", self.next);
        self.jobs.push_back(Job {
            id: new_id.clone(),
            input: old.input,
            output: old.output,
            renamed: old.renamed,
            preset: old.preset,
            limits: old.limits,
            password: None,
            unlocked: false,
            rebuild: Some(rebuild),
            phase: Phase::Queued,
        });
        self.pump();
        self.announce_all();
        Ok(new_id)
    }

    /// Whether a job not yet finished will write `path`.
    fn reserves(&self, path: &Path) -> bool {
        self.jobs
            .iter()
            .any(|job| job.output == path && !matches!(job.phase, Phase::Done(_)))
    }

    /// Cancel a job: a waiting one leaves the queue without starting, a running one is sent
    /// `{"t":"cancel"}` and killed if it has not exited `ipc.kill_after_secs` later.
    pub fn cancel(&mut self, id: &str, now: Instant) -> Result<(), UiError> {
        let job = self
            .jobs
            .iter_mut()
            .find(|job| job.id == id)
            .ok_or_else(|| UiError::UnknownJob(id.to_owned()))?;
        let phase = std::mem::replace(&mut job.phase, Phase::Done(JobState::Running));
        job.phase = match phase {
            Phase::Queued => Phase::Done(JobState::CancelledBeforeStart),
            Phase::Running(mut running) => {
                running.cancel()?;
                Phase::Cancelling {
                    running,
                    asked: now,
                    killed: false,
                }
            }
            other => other,
        };
        self.pump();
        self.announce_all();
        Ok(())
    }

    /// Remove a job that is waiting or finished. A running job has to be cancelled first, so a
    /// row never disappears with an engine still behind it.
    pub fn remove(&mut self, id: &str) -> Result<(), UiError> {
        let index = self
            .jobs
            .iter()
            .position(|job| job.id == id)
            .ok_or_else(|| UiError::UnknownJob(id.to_owned()))?;
        if matches!(
            self.jobs[index].phase,
            Phase::Running(_) | Phase::Cancelling { .. }
        ) {
            return Err(UiError::Io(format!("{id} is running; cancel it first")));
        }
        self.jobs.remove(index);
        self.announce_all();
        Ok(())
    }

    /// Notice exits, enforce the kill deadline, and start what may start.
    pub fn tick(&mut self, now: Instant) {
        let mut changed = false;
        for job in &mut self.jobs {
            let phase = std::mem::replace(&mut job.phase, Phase::Done(JobState::Running));
            job.phase = match phase {
                Phase::Running(mut running) => match running.try_wait() {
                    Ok(Some(code)) => {
                        changed = true;
                        Phase::Done(JobState::Exited { code })
                    }
                    Ok(None) => Phase::Running(running),
                    Err(error) => {
                        changed = true;
                        Phase::Done(JobState::FailedToStart { error })
                    }
                },
                Phase::Cancelling {
                    mut running,
                    asked,
                    mut killed,
                } => match running.try_wait() {
                    Ok(Some(code)) => {
                        changed = true;
                        Phase::Done(JobState::Exited { code })
                    }
                    _ => {
                        // D13.2: the engine has two seconds to say `done{cancelled}` and five to
                        // be gone; after that the supervisor ends it.
                        if !killed && now.duration_since(asked) >= self.kill_after {
                            let _ = running.kill();
                            killed = true;
                        }
                        Phase::Cancelling {
                            running,
                            asked,
                            killed,
                        }
                    }
                },
                other => other,
            };
        }
        if self.pump() || changed {
            self.announce_all();
        }
    }

    /// Start waiting jobs while fewer than `max_concurrent` run. Returns whether any started.
    fn pump(&mut self) -> bool {
        let mut started = false;
        loop {
            let active = self
                .jobs
                .iter()
                .filter(|job| matches!(job.phase, Phase::Running(_) | Phase::Cancelling { .. }))
                .count();
            if active >= self.max_concurrent {
                return started;
            }
            let Some(job) = self
                .jobs
                .iter_mut()
                .find(|job| matches!(job.phase, Phase::Queued))
            else {
                return started;
            };
            let mut spec = JobSpec::new(job.input.clone(), job.output.clone());
            spec.job_id = Some(job.id.clone());
            spec.preset = Some(job.preset);
            spec.limits = job.limits;
            if let Some(rebuild) = &job.rebuild {
                // The rebuild replaces the book this queue wrote, the one the user corrected.
                spec.output.overwrite = true;
                spec.input.sha256 = Some(rebuild.sha256.clone());
                spec.overrides_path = Some(rebuild.overrides.clone());
            }
            let sink = Arc::clone(&self.sink);
            let logs = Arc::clone(&self.logs);
            let id = job.id.clone();
            let on_line = Box::new(move |line: String| {
                logs.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .entry(id.clone())
                    .or_default()
                    .push(line.clone());
                sink.line(&id, line);
            });
            let password = job.password.take();
            job.phase = match self
                .engine
                .start(&job.id, &spec, password.as_deref(), on_line)
            {
                Ok(running) => Phase::Running(running),
                Err(error) => Phase::Done(JobState::FailedToStart { error }),
            };
            started = true;
        }
    }

    /// The EPUB job `id` writes, and the report beside it — the only paths the app will open,
    /// reveal or read on the webview's behalf, looked up by id so the webview never names one.
    pub fn outputs(&self, id: &str) -> Result<(PathBuf, PathBuf), UiError> {
        let job = self
            .jobs
            .iter()
            .find(|job| job.id == id)
            .ok_or_else(|| UiError::UnknownJob(id.to_owned()))?;
        let mut report = job.output.as_os_str().to_os_string();
        report.push(".report.json");
        Ok((job.output.clone(), PathBuf::from(report)))
    }

    /// The event lines job `id`'s engine has written so far.
    pub fn events(&self, id: &str) -> Vec<String> {
        self.logs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(id)
            .cloned()
            .unwrap_or_default()
    }

    /// Every row, in queue order, with waiting positions counted from the running job as #1.
    pub fn views(&self) -> Vec<JobView> {
        let mut position = self
            .jobs
            .iter()
            .filter(|job| matches!(job.phase, Phase::Running(_) | Phase::Cancelling { .. }))
            .count();
        self.jobs
            .iter()
            .map(|job| {
                let state = match &job.phase {
                    Phase::Queued => {
                        position += 1;
                        JobState::Queued { position }
                    }
                    Phase::Running(_) => JobState::Running,
                    Phase::Cancelling { .. } => JobState::Cancelling,
                    Phase::Done(state) => state.clone(),
                };
                JobView {
                    id: job.id.clone(),
                    input: job.input.clone(),
                    output: job.output.clone(),
                    renamed: job.renamed,
                    unlocked: job.unlocked,
                    rebuild: job.rebuild.is_some(),
                    state,
                }
            })
            .collect()
    }

    fn announce_all(&self) {
        for view in self.views() {
            self.sink.changed(&view);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::engine::LineSink;
    use crate::fs_scope::AppDirs;

    /// What a fake engine has been asked to do, and whether it has exited.
    #[derive(Default)]
    struct Script {
        launched: Vec<PathBuf>,
        cancelled: Vec<usize>,
        killed: Vec<usize>,
        /// Per launch: `None` while running, then the exit as `try_wait` reports it.
        exited: Vec<Option<Option<i32>>>,
        /// Per launch: the password it was given.
        passwords: Vec<Option<String>>,
    }

    #[derive(Clone, Default)]
    struct FakeLauncher(Arc<Mutex<Script>>);

    struct FakeRunning {
        index: usize,
        script: Arc<Mutex<Script>>,
    }

    impl Launch for FakeLauncher {
        fn launch(
            &self,
            spec: &Path,
            password: Option<&str>,
            _on_line: LineSink,
        ) -> Result<Box<dyn Running>, UiError> {
            let mut script = self.0.lock().expect("not poisoned");
            script.launched.push(spec.to_path_buf());
            script.passwords.push(password.map(str::to_owned));
            script.exited.push(None);
            Ok(Box::new(FakeRunning {
                index: script.launched.len() - 1,
                script: Arc::clone(&self.0),
            }))
        }
    }

    impl Running for FakeRunning {
        fn cancel(&mut self) -> Result<(), UiError> {
            self.script
                .lock()
                .expect("not poisoned")
                .cancelled
                .push(self.index);
            Ok(())
        }
        fn kill(&mut self) -> Result<(), UiError> {
            let mut script = self.script.lock().expect("not poisoned");
            script.killed.push(self.index);
            script.exited[self.index] = Some(None);
            Ok(())
        }
        fn try_wait(&mut self) -> Result<Option<Option<i32>>, UiError> {
            Ok(self.script.lock().expect("not poisoned").exited[self.index])
        }
    }

    #[derive(Default)]
    struct Recorded(Mutex<Vec<JobView>>);
    impl QueueSink for Recorded {
        fn line(&self, _job: &str, _line: String) {}
        fn changed(&self, job: &JobView) {
            self.0.lock().expect("not poisoned").push(job.clone());
        }
    }

    fn queue(name: &str) -> (JobQueue<FakeLauncher>, FakeLauncher) {
        let dir =
            std::env::temp_dir().join(format!("oc-desktop-queue-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let dirs = AppDirs::under(&dir).expect("made");
        let launcher = FakeLauncher::default();
        let engine = Engine::new(dirs.jobs, launcher.clone());
        (
            JobQueue::new(engine, Arc::new(Recorded::default())),
            launcher,
        )
    }

    fn running(queue: &JobQueue<FakeLauncher>) -> usize {
        queue
            .views()
            .iter()
            .filter(|view| matches!(view.state, JobState::Running | JobState::Cancelling))
            .count()
    }

    /// 12.7 / A12.1 — forty dropped files are forty jobs, exactly one running, the rest waiting in
    /// drop order; and it stays one as jobs finish.
    #[test]
    fn queue_runs_one_job_at_a_time() {
        let (mut queue, launcher) = queue("forty");
        let drop: Vec<PathBuf> = (1..=40)
            .map(|n| PathBuf::from(format!("/books/book-{n:02}.pdf")))
            .collect();
        let ids = queue.enqueue(&drop, PresetName::Auto);

        assert_eq!(ids.len(), 40, "one job per file");
        let views = queue.views();
        assert_eq!(views.len(), 40);
        assert_eq!(running(&queue), 1, "exactly one running");
        assert_eq!(launcher.0.lock().expect("not poisoned").launched.len(), 1);
        for (index, view) in views.iter().enumerate().skip(1) {
            assert_eq!(
                view.state,
                JobState::Queued {
                    position: index + 1
                },
                "waiting in drop order, the running job counted as #1"
            );
            assert_eq!(view.input, drop[index]);
        }

        // Finish jobs one by one: the next starts, and never two at once.
        for finished in 0..39 {
            launcher.0.lock().expect("not poisoned").exited[finished] = Some(Some(0));
            queue.tick(Instant::now());
            assert_eq!(
                running(&queue),
                1,
                "still exactly one after {finished} finished"
            );
            assert_eq!(
                launcher.0.lock().expect("not poisoned").launched.len(),
                finished + 2
            );
        }
    }

    #[test]
    fn a_waiting_job_is_cancelled_without_ever_starting() {
        let (mut queue, launcher) = queue("waiting");
        let ids = queue.enqueue(
            &[PathBuf::from("/b/a.pdf"), PathBuf::from("/b/b.pdf")],
            PresetName::Auto,
        );
        queue.cancel(&ids[1], Instant::now()).expect("known");
        assert_eq!(queue.views()[1].state, JobState::CancelledBeforeStart);
        launcher.0.lock().expect("not poisoned").exited[0] = Some(Some(0));
        queue.tick(Instant::now());
        assert_eq!(
            launcher.0.lock().expect("not poisoned").launched.len(),
            1,
            "the cancelled job never launched"
        );
        queue
            .remove(&ids[1])
            .expect("a finished job can be removed");
        assert_eq!(queue.views().len(), 1);
    }

    /// D13.2 — cancel is a message first and a kill only after `ipc.kill_after_secs`.
    #[test]
    fn a_running_cancel_escalates_to_a_kill_at_the_deadline() {
        let (mut queue, launcher) = queue("kill");
        let ids = queue.enqueue(&[PathBuf::from("/b/slow.pdf")], PresetName::Auto);
        let asked = Instant::now();
        queue.cancel(&ids[0], asked).expect("known");
        assert_eq!(queue.views()[0].state, JobState::Cancelling);
        assert_eq!(launcher.0.lock().expect("not poisoned").cancelled, [0]);
        assert!(
            queue.remove(&ids[0]).is_err(),
            "a running row cannot vanish"
        );

        let kill_after =
            Duration::from_secs(u64::try_from(T.ipc.kill_after_secs).expect("positive"));
        queue.tick(asked + kill_after - Duration::from_millis(1));
        assert!(launcher.0.lock().expect("not poisoned").killed.is_empty());
        queue.tick(asked + kill_after);
        assert_eq!(launcher.0.lock().expect("not poisoned").killed, [0]);
        queue.tick(asked + kill_after);
        assert_eq!(queue.views()[0].state, JobState::Exited { code: None });
    }

    #[test]
    fn an_unlocked_job_gets_its_password_once_and_keeps_none() {
        let (mut queue, launcher) = queue("unlock");
        let ids = queue.enqueue(&[PathBuf::from("/b/locked.pdf")], PresetName::Auto);
        assert_eq!(
            queue.unlock(&ids[0], "early".to_owned()),
            Err(UiError::JobBusy(ids[0].clone())),
            "a running job is not restarted under it"
        );
        launcher.0.lock().expect("not poisoned").exited[0] = Some(Some(2));
        queue.tick(Instant::now());

        let again = queue.unlock(&ids[0], "hunter22".to_owned()).expect("known");
        let script = launcher.0.lock().expect("not poisoned");
        assert_eq!(script.passwords, [None, Some("hunter22".to_owned())]);
        drop(script);
        assert_eq!(
            queue.views().len(),
            1,
            "the locked row is replaced, not duplicated"
        );
        assert_eq!(queue.views()[0].id, again);
        assert!(
            queue.views()[0].unlocked,
            "the row knows a password was given"
        );
        assert!(
            queue.jobs.iter().all(|job| job.password.is_none()),
            "the password is dropped once the engine has it"
        );
    }

    /// "Fix and rebuild" replaces the row with a job that names the corrections, the digest they
    /// were made for, and permission to replace the book it wrote — and only once that book is
    /// written.
    #[test]
    fn a_rebuild_replaces_the_row_with_the_corrections_named() {
        let (mut queue, launcher) = queue("rebuild");
        let ids = queue.enqueue(&[PathBuf::from("/b/novel.pdf")], PresetName::Novel);
        let rebuild = Rebuild {
            overrides: PathBuf::from("/data/overrides/abc.json"),
            sha256: "ab".repeat(32),
        };
        assert_eq!(
            queue.rebuild(&ids[0], rebuild.clone()),
            Err(UiError::JobBusy(ids[0].clone())),
            "not while it is still converting"
        );
        launcher.0.lock().expect("not poisoned").exited[0] = Some(Some(0));
        queue.tick(Instant::now());

        let again = queue.rebuild(&ids[0], rebuild.clone()).expect("rebuilt");
        let views = queue.views();
        assert_eq!(views.len(), 1, "the row is replaced, not duplicated");
        assert_eq!(views[0].id, again);
        assert!(views[0].rebuild);
        assert_eq!(views[0].output, PathBuf::from("/b/novel.epub"));

        let script = launcher.0.lock().expect("not poisoned");
        let spec: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&script.launched[1]).expect("the spec is on disk"),
        )
        .expect("JSON");
        assert_eq!(spec["overrides_path"], "/data/overrides/abc.json");
        assert_eq!(spec["input"]["sha256"], rebuild.sha256);
        assert_eq!(spec["output"]["path"], "/b/novel.epub");
        assert_eq!(
            spec["output"]["overwrite"], true,
            "it replaces the book it wrote"
        );
        assert_eq!(spec["preset"], "novel");
    }

    #[test]
    fn two_books_with_one_name_get_two_outputs() {
        let (mut queue, _) = queue("names");
        queue.enqueue(
            &[PathBuf::from("/x/Book.pdf"), PathBuf::from("/x/Book.PDF")],
            PresetName::Auto,
        );
        let views = queue.views();
        assert_eq!(views[0].output, PathBuf::from("/x/Book.epub"));
        assert_eq!(views[1].output, PathBuf::from("/x/Book (2).epub"));
        assert!(views[1].renamed);
    }
}
