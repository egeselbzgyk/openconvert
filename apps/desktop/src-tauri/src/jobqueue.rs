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
//!
//! Every job that ends is reported once to the sink ([`QueueSink::finished`]), which is how the
//! app's history (`history.rs`) learns of it, and a book is written beside its PDF or into the
//! library folder the settings name ([`JobQueue::enqueue_into`]).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant, SystemTime};

use oc_core::jobspec::{AiMode, JobSpec, LimitsSpec};
use oc_core::thresholds::T;
use oc_model::document::PresetName;
use serde::Serialize;

use crate::ai::{AiUnavailable, AiView, JobAi, ModelServer, ServerLease};
use crate::engine::{Engine, Launch, Running, UiError};
use crate::fs_scope::{default_output_for, free_output_path_among, output_in};

/// A job's state as the queue knows it. What happens *inside* a run — stages, progress,
/// heartbeats — is the engine's events, which go straight to the UI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum JobState {
    /// Waiting; `position` counts the running job as #1 (design decision 6).
    Queued {
        position: usize,
    },
    /// Its turn has come and it waits for the app's model server to load (AI assistance, built-in).
    Preparing,
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
    /// AI assistance for this job: the provider, and why it could not be used when it could not.
    /// `None` with AI off.
    pub ai: Option<AiView>,
    #[serde(flatten)]
    pub state: JobState,
}

/// A job that has ended, reported once ([`QueueSink::finished`]): what the app's history records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finished {
    pub id: String,
    pub input: PathBuf,
    pub output: PathBuf,
    /// The report the engine writes beside the output.
    pub report: PathBuf,
    /// When its engine started; `None` for a job that never started one.
    pub started: Option<SystemTime>,
    pub ended: SystemTime,
    /// It ended because the user cancelled it.
    pub cancelled: bool,
    /// How it ended: `Exited`, `FailedToStart` or `CancelledBeforeStart`.
    pub state: JobState,
}

/// What the queue tells the app, which relays it to the webview.
pub trait QueueSink: Send + Sync {
    /// A line of NDJSON from job `job`'s engine.
    fn line(&self, job: &str, line: String);
    /// Job `job` changed state outside its event stream (started, exited, removed).
    fn changed(&self, job: &JobView);
    /// Job `job` has ended. Called once per job, after its last state change.
    fn finished(&self, _job: &Finished) {}
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
    /// AI assistance as the settings were when it was queued.
    ai: JobAi,
    /// It asked for AI assistance and converts without it, for this reason.
    ai_unavailable: Option<AiUnavailable>,
    /// It holds a lease on the app's model server, to be released when it ends.
    leased: bool,
    /// When its engine started.
    started: Option<SystemTime>,
    /// Cancel was pressed on it.
    cancel_requested: bool,
    /// Its end has been reported ([`QueueSink::finished`]).
    reported: bool,
    phase: Phase,
}

impl Job {
    /// A job for `input` that writes `output`, waiting its turn.
    fn waiting(
        id: String,
        input: PathBuf,
        output: PathBuf,
        renamed: bool,
        preset: PresetName,
        limits: Option<LimitsSpec>,
        ai: JobAi,
    ) -> Self {
        Self {
            id,
            input,
            output,
            renamed,
            preset,
            limits,
            password: None,
            unlocked: false,
            rebuild: None,
            ai,
            ai_unavailable: None,
            leased: false,
            started: None,
            cancel_requested: false,
            reported: false,
            phase: Phase::Queued,
        }
    }
}

/// The report the engine writes beside `output` (`<output>.report.json`).
fn report_beside(output: &Path) -> PathBuf {
    let mut report = output.as_os_str().to_os_string();
    report.push(".report.json");
    PathBuf::from(report)
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
    /// Waiting for the app's model server: the lease arrives on `lease` from the thread that
    /// started it, and its model is asked in `mode`. `cancelled` once Cancel was pressed meanwhile:
    /// the job then ends without starting.
    Preparing {
        lease: mpsc::Receiver<Result<ServerLease, AiUnavailable>>,
        mode: AiMode,
        cancelled: bool,
    },
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
    /// The app's own model server, for jobs with built-in AI assistance. `None`: this build has
    /// none, and such a job converts without AI.
    server: Option<Arc<dyn ModelServer>>,
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
            server: None,
        }
    }

    /// Lease `server` to the jobs that want built-in AI assistance.
    pub fn with_model_server(mut self, server: Arc<dyn ModelServer>) -> Self {
        self.server = Some(server);
        self
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
        self.enqueue_as(pdfs, preset, limits, &JobAi::Off)
    }

    /// [`JobQueue::enqueue_with`], with AI assistance as the settings say ([`JobAi::plan`]).
    pub fn enqueue_as(
        &mut self,
        pdfs: &[PathBuf],
        preset: PresetName,
        limits: Option<LimitsSpec>,
        ai: &JobAi,
    ) -> Vec<String> {
        self.enqueue_into(pdfs, preset, limits, ai, None)
    }

    /// [`JobQueue::enqueue_as`], saving every book in `folder` — the library — rather than beside
    /// its PDF when one is given.
    pub fn enqueue_into(
        &mut self,
        pdfs: &[PathBuf],
        preset: PresetName,
        limits: Option<LimitsSpec>,
        ai: &JobAi,
        folder: Option<&Path>,
    ) -> Vec<String> {
        let mut ids = Vec::with_capacity(pdfs.len());
        for input in pdfs {
            self.next += 1;
            let id = format!("job-{}", self.next);
            let desired = match folder {
                Some(folder) => output_in(folder, input),
                None => default_output_for(input),
            };
            let output = free_output_path_among(&desired, |path| self.reserves(path));
            let renamed = output != desired;
            self.jobs.push_back(Job::waiting(
                id.clone(),
                input.clone(),
                output,
                renamed,
                preset,
                limits,
                ai.clone(),
            ));
            ids.push(id);
        }
        self.pump();
        self.settle();
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
            password: Some(password),
            unlocked: true,
            // A locked book being rebuilt keeps its corrections through the unlock.
            rebuild: old.rebuild,
            ..Job::waiting(
                new_id.clone(),
                old.input,
                old.output,
                old.renamed,
                old.preset,
                old.limits,
                old.ai,
            )
        });
        self.pump();
        self.settle();
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
            rebuild: Some(rebuild),
            ..Job::waiting(
                new_id.clone(),
                old.input,
                old.output,
                old.renamed,
                old.preset,
                old.limits,
                old.ai,
            )
        });
        self.pump();
        self.settle();
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
        if !matches!(job.phase, Phase::Done(_)) {
            job.cancel_requested = true;
        }
        let phase = std::mem::replace(&mut job.phase, Phase::Done(JobState::Running));
        job.phase = match phase {
            Phase::Queued => Phase::Done(JobState::CancelledBeforeStart),
            // The server keeps loading; the job ends when its lease arrives, without starting.
            Phase::Preparing { lease, mode, .. } => Phase::Preparing {
                lease,
                mode,
                cancelled: true,
            },
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
        self.settle();
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
            Phase::Preparing { .. } | Phase::Running(_) | Phase::Cancelling { .. }
        ) {
            return Err(UiError::Io(format!("{id} is running; cancel it first")));
        }
        self.jobs.remove(index);
        self.announce_all();
        Ok(())
    }

    /// Notice exits, enforce the kill deadline, start the jobs whose model server is ready, and start
    /// what may start.
    pub fn tick(&mut self, now: Instant) {
        let mut changed = false;
        for index in 0..self.jobs.len() {
            let phase =
                std::mem::replace(&mut self.jobs[index].phase, Phase::Done(JobState::Running));
            let next = match phase {
                Phase::Preparing {
                    lease,
                    mode,
                    cancelled,
                } => match lease.try_recv() {
                    Err(mpsc::TryRecvError::Empty) => Phase::Preparing {
                        lease,
                        mode,
                        cancelled,
                    },
                    arrived => {
                        changed = true;
                        let arrived = arrived.unwrap_or(Err(AiUnavailable::ServerFailed));
                        if cancelled {
                            if arrived.is_ok() {
                                self.release_server();
                            }
                            Phase::Done(JobState::CancelledBeforeStart)
                        } else {
                            let job = &mut self.jobs[index];
                            let ai = match arrived {
                                Ok(lease) => {
                                    job.leased = true;
                                    Some(lease.spec(mode))
                                }
                                Err(why) => {
                                    job.ai_unavailable = Some(why);
                                    None
                                }
                            };
                            self.launch(index, ai)
                        }
                    }
                },
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
            let ended = matches!(next, Phase::Done(_));
            self.jobs[index].phase = next;
            if ended && self.jobs[index].leased {
                self.jobs[index].leased = false;
                self.release_server();
            }
        }
        if self.pump() || changed {
            self.settle();
        }
    }

    /// Report every job that has ended and not been reported, then announce every row.
    fn settle(&mut self) {
        let ended = SystemTime::now();
        for job in &mut self.jobs {
            let Phase::Done(state) = &job.phase else {
                continue;
            };
            if job.reported {
                continue;
            }
            job.reported = true;
            self.sink.finished(&Finished {
                id: job.id.clone(),
                input: job.input.clone(),
                output: job.output.clone(),
                report: report_beside(&job.output),
                started: job.started,
                ended,
                cancelled: job.cancel_requested,
                state: state.clone(),
            });
        }
        self.announce_all();
    }

    fn release_server(&self) {
        if let Some(server) = &self.server {
            server.release();
        }
    }

    /// Start waiting jobs while fewer than `max_concurrent` are under way. Returns whether any
    /// started — or began waiting for the model server, which counts as under way.
    fn pump(&mut self) -> bool {
        let mut started = false;
        loop {
            let active = self
                .jobs
                .iter()
                .filter(|job| {
                    matches!(
                        job.phase,
                        Phase::Preparing { .. } | Phase::Running(_) | Phase::Cancelling { .. }
                    )
                })
                .count();
            if active >= self.max_concurrent {
                return started;
            }
            let Some(index) = self
                .jobs
                .iter()
                .position(|job| matches!(job.phase, Phase::Queued))
            else {
                return started;
            };
            started = true;
            let phase = match self.jobs[index].ai.clone() {
                JobAi::Off => self.launch(index, None),
                JobAi::Endpoint { spec, .. } => self.launch(index, Some(spec)),
                JobAi::Unusable { .. } => self.launch(index, None),
                // D10: nothing is sent to a host nobody consented to, so nothing is started; the
                // UI opens the consent dialog again.
                JobAi::ConsentRequired { host } => Phase::Done(JobState::FailedToStart {
                    error: UiError::ConsentRequired { host },
                }),
                JobAi::Builtin { mode } => match &self.server {
                    // The model loads off the queue's lock: a gigabyte can take a while, and the
                    // queue must keep answering Cancel meanwhile.
                    Some(server) => {
                        let server = Arc::clone(server);
                        let (send, lease) = mpsc::channel();
                        std::thread::spawn(move || {
                            let _ = send.send(server.acquire());
                        });
                        Phase::Preparing {
                            lease,
                            mode,
                            cancelled: false,
                        }
                    }
                    None => {
                        self.jobs[index].ai_unavailable = Some(AiUnavailable::NoServer);
                        self.launch(index, None)
                    }
                },
            };
            self.jobs[index].phase = phase;
        }
    }

    /// Write job `index`'s spec, with `ai` as its `ai` object, and start its engine.
    fn launch(&mut self, index: usize, ai: Option<oc_core::jobspec::AiSpec>) -> Phase {
        let job = &mut self.jobs[index];
        let mut spec = JobSpec::new(job.input.clone(), job.output.clone());
        spec.job_id = Some(job.id.clone());
        spec.preset = Some(job.preset);
        spec.limits = job.limits;
        spec.ai = ai;
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
        match self
            .engine
            .start(&job.id, &spec, password.as_deref(), on_line)
        {
            Ok(running) => {
                job.started = Some(SystemTime::now());
                Phase::Running(running)
            }
            Err(error) => {
                if job.leased {
                    job.leased = false;
                    if let Some(server) = &self.server {
                        server.release();
                    }
                }
                Phase::Done(JobState::FailedToStart { error })
            }
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
        Ok((job.output.clone(), report_beside(&job.output)))
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
            .filter(|job| {
                matches!(
                    job.phase,
                    Phase::Preparing { .. } | Phase::Running(_) | Phase::Cancelling { .. }
                )
            })
            .count();
        self.jobs
            .iter()
            .map(|job| {
                let state = match &job.phase {
                    Phase::Queued => {
                        position += 1;
                        JobState::Queued { position }
                    }
                    Phase::Preparing {
                        cancelled: false, ..
                    } => JobState::Preparing,
                    Phase::Preparing {
                        cancelled: true, ..
                    } => JobState::Cancelling,
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
                    ai: job.ai.view(job.ai_unavailable),
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

    /// The app's model server, scripted: each acquire answers `answer`, after `gate` opens when
    /// there is one.
    struct FakeServer {
        answer: Result<ServerLease, AiUnavailable>,
        gate: Mutex<Option<mpsc::Receiver<()>>>,
        acquired: std::sync::atomic::AtomicUsize,
        released: std::sync::atomic::AtomicUsize,
    }

    impl FakeServer {
        fn answering(answer: Result<ServerLease, AiUnavailable>) -> Arc<Self> {
            Arc::new(Self {
                answer,
                gate: Mutex::new(None),
                acquired: Default::default(),
                released: Default::default(),
            })
        }
        fn released(&self) -> usize {
            self.released.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    impl ModelServer for FakeServer {
        fn acquire(&self) -> Result<ServerLease, AiUnavailable> {
            let gate = self.gate.lock().expect("not poisoned").take();
            if let Some(gate) = gate {
                let _ = gate.recv();
            }
            self.acquired
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.answer.clone()
        }
        fn release(&self) {
            self.released
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    fn lease() -> ServerLease {
        ServerLease {
            endpoint: "http://127.0.0.1:40123".to_owned(),
            api_key_file: PathBuf::from("/data/run/llm.key"),
            model_id: "qwen3-1.7b".to_owned(),
        }
    }

    /// Tick until the first job has left `Preparing`, as the app's supervisor clock would.
    fn until_prepared(queue: &mut JobQueue<FakeLauncher>) {
        until(queue, |state| *state != JobState::Preparing);
    }

    fn until(queue: &mut JobQueue<FakeLauncher>, done: impl Fn(&JobState) -> bool) {
        for _ in 0..500 {
            queue.tick(Instant::now());
            if done(&queue.views()[0].state) {
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("the lease never arrived");
    }

    fn spec_of(launcher: &FakeLauncher, launch: usize) -> serde_json::Value {
        let script = launcher.0.lock().expect("not poisoned");
        serde_json::from_str(&std::fs::read_to_string(&script.launched[launch]).expect("on disk"))
            .expect("JSON")
    }

    /// Built-in AI: the job waits for the app's server, its spec names that server — endpoint, the
    /// run's key file, the model — and the lease is released when the job ends, so the server's
    /// idle clock can start (D8, PHASE 9 detail 3).
    #[test]
    fn built_in_ai_leases_the_app_server_for_the_job_and_releases_it_at_the_end() {
        let (queue, launcher) = queue("ai-lease");
        let server = FakeServer::answering(Ok(lease()));
        let mut queue = queue.with_model_server(server.clone());
        queue.enqueue_as(
            &[PathBuf::from("/b/a.pdf"), PathBuf::from("/b/b.pdf")],
            PresetName::Auto,
            None,
            &JobAi::Builtin {
                mode: AiMode::Quality,
            },
        );
        assert_eq!(queue.views()[1].state, JobState::Queued { position: 2 });
        until_prepared(&mut queue);
        assert_eq!(queue.views()[0].state, JobState::Running);
        assert_eq!(
            queue.views()[0].ai,
            Some(AiView {
                provider: crate::settings::Provider::Builtin,
                unavailable: None
            })
        );
        let spec = spec_of(&launcher, 0);
        assert_eq!(spec["ai"]["enabled"], true);
        assert_eq!(spec["ai"]["endpoint"], "http://127.0.0.1:40123");
        assert_eq!(spec["ai"]["api_key_file"], "/data/run/llm.key");
        assert_eq!(spec["ai"]["model_id"], "qwen3-1.7b");
        assert_eq!(server.released(), 0, "held while the job runs");

        launcher.0.lock().expect("not poisoned").exited[0] = Some(Some(0));
        queue.tick(Instant::now());
        assert_eq!(server.released(), 1, "released when the job ends");
    }

    /// UI_UX §4's fail-open: no model installed (or no server, or one that did not come up) — the
    /// book converts without AI, and the row says why.
    #[test]
    fn built_in_ai_without_a_model_converts_without_ai_and_says_why() {
        let (queue, launcher) = queue("ai-nomodel");
        let mut queue = queue.with_model_server(FakeServer::answering(Err(AiUnavailable::NoModel)));
        queue.enqueue_as(
            &[PathBuf::from("/b/a.pdf")],
            PresetName::Auto,
            None,
            &JobAi::Builtin {
                mode: AiMode::Quality,
            },
        );
        until_prepared(&mut queue);
        assert_eq!(queue.views()[0].state, JobState::Running);
        assert_eq!(
            queue.views()[0].ai.as_ref().and_then(|ai| ai.unavailable),
            Some(AiUnavailable::NoModel)
        );
        assert!(spec_of(&launcher, 0).get("ai").is_none(), "no ai object");

        let (mut queue, launcher) = queue_named("ai-noserver");
        queue.enqueue_as(
            &[PathBuf::from("/b/a.pdf")],
            PresetName::Auto,
            None,
            &JobAi::Builtin {
                mode: AiMode::Quality,
            },
        );
        assert_eq!(queue.views()[0].state, JobState::Running);
        assert_eq!(
            queue.views()[0].ai.as_ref().and_then(|ai| ai.unavailable),
            Some(AiUnavailable::NoServer),
            "a build without the server says so"
        );
        assert!(spec_of(&launcher, 0).get("ai").is_none());
    }

    /// Cancel while the model loads: the job never starts, and the server it was waiting for is
    /// released as soon as it is up.
    #[test]
    fn a_job_cancelled_while_the_model_loads_never_starts() {
        let (queue, launcher) = queue("ai-cancel");
        let server = FakeServer::answering(Ok(lease()));
        let (open, gate) = mpsc::channel();
        *server.gate.lock().expect("not poisoned") = Some(gate);
        let mut queue = queue.with_model_server(server.clone());
        let ids = queue.enqueue_as(
            &[PathBuf::from("/b/a.pdf")],
            PresetName::Auto,
            None,
            &JobAi::Builtin {
                mode: AiMode::Quality,
            },
        );
        assert_eq!(queue.views()[0].state, JobState::Preparing);
        assert!(
            queue.remove(&ids[0]).is_err(),
            "a preparing row cannot vanish"
        );
        queue.cancel(&ids[0], Instant::now()).expect("known");
        assert_eq!(queue.views()[0].state, JobState::Cancelling);

        open.send(()).expect("the gate opens");
        until(&mut queue, |state| *state != JobState::Cancelling);
        assert_eq!(queue.views()[0].state, JobState::CancelledBeforeStart);
        assert!(launcher.0.lock().expect("not poisoned").launched.is_empty());
        assert_eq!(server.released(), 1);
    }

    /// D10: a host nobody consented to is never sent anything — the job does not start, and says
    /// which host needs consent.
    #[test]
    fn a_host_nobody_consented_to_never_starts() {
        let (mut queue, launcher) = queue("ai-consent");
        queue.enqueue_as(
            &[PathBuf::from("/b/a.pdf")],
            PresetName::Auto,
            None,
            &JobAi::ConsentRequired {
                host: "llm.example.org".to_owned(),
            },
        );
        assert_eq!(
            queue.views()[0].state,
            JobState::FailedToStart {
                error: UiError::ConsentRequired {
                    host: "llm.example.org".to_owned()
                }
            }
        );
        assert!(launcher.0.lock().expect("not poisoned").launched.is_empty());
    }

    fn queue_named(name: &str) -> (JobQueue<FakeLauncher>, FakeLauncher) {
        queue(name)
    }

    /// Every finished job, as the queue reported it.
    #[derive(Default)]
    struct Finishes(Mutex<Vec<Finished>>);
    impl QueueSink for Finishes {
        fn line(&self, _job: &str, _line: String) {}
        fn changed(&self, _job: &JobView) {}
        fn finished(&self, job: &Finished) {
            self.0.lock().expect("not poisoned").push(job.clone());
        }
    }

    /// A queue that records what finished, and a directory of absolute paths on this host (a
    /// spec's paths must be absolute, and `/b/a.pdf` is not on Windows).
    fn recording(name: &str) -> (JobQueue<FakeLauncher>, FakeLauncher, Arc<Finishes>, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("oc-desktop-queue-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let dirs = AppDirs::under(&dir.join("app")).expect("made");
        let launcher = FakeLauncher::default();
        let finishes = Arc::new(Finishes::default());
        let queue = JobQueue::new(Engine::new(dirs.jobs, launcher.clone()), finishes.clone());
        (queue, launcher, finishes, dir)
    }

    /// The stage deadline the settings give (the app's default, 30 minutes) reaches the spec the
    /// engine reads, with the other limits.
    #[test]
    fn a_job_spec_carries_the_stage_deadline_it_was_queued_with() {
        let (mut queue, launcher, _, dir) = recording("deadline");
        let limits = crate::settings::Settings::default().limits();
        queue.enqueue_into(
            &[dir.join("big.pdf")],
            PresetName::Auto,
            Some(limits),
            &JobAi::Off,
            None,
        );
        let spec = spec_of(&launcher, 0);
        assert_eq!(
            spec["limits"]["stage_deadline_secs"],
            u64::try_from(T.desktop.default_stage_deadline_secs).expect("positive")
        );
        assert!(
            spec["limits"].get("max_pages").is_none(),
            "unchanged caps are not sent"
        );
    }

    /// The AI mode a job was queued with is its spec's `ai.mode` (job spec v2), whichever provider
    /// answers: the app's own server, once its lease arrives, and an endpoint as the settings
    /// configure it — Ollama or a custom one.
    #[test]
    fn a_job_spec_carries_the_ai_mode_it_was_queued_with() {
        use crate::settings::{Provider, Settings};

        let (queue, launcher, _, dir) = recording("ai-mode");
        let server = FakeServer::answering(Ok(ServerLease {
            endpoint: "http://127.0.0.1:40123".to_owned(),
            api_key_file: dir.join("run").join("llm.key"),
            model_id: "qwen3-1.7b".to_owned(),
        }));
        let mut queue = queue.with_model_server(server);
        let settings = |provider, ai_mode| {
            let mut settings = Settings {
                ai_enabled: true,
                provider,
                ai_mode,
                ..Settings::default()
            };
            settings.custom.endpoint = "http://127.0.0.1:1234/v1".to_owned();
            settings
        };
        let planned = [
            (Provider::Builtin, AiMode::Fast),
            (Provider::Ollama, AiMode::Quality),
            (Provider::Custom, AiMode::Fast),
        ];
        for (index, (provider, mode)) in planned.into_iter().enumerate() {
            queue.enqueue_into(
                &[dir.join(format!("{index}.pdf"))],
                PresetName::Auto,
                None,
                &JobAi::plan(&settings(provider, mode)),
                None,
            );
        }
        until_prepared(&mut queue);
        for (index, (provider, mode)) in planned.into_iter().enumerate() {
            let spec = spec_of(&launcher, index);
            assert_eq!(spec["schema"], "openconvert.job/2", "{provider:?}");
            assert_eq!(spec["ai"]["enabled"], true, "{provider:?}");
            assert_eq!(spec["ai"]["mode"], mode.as_str(), "{provider:?}");
            // One job at a time: the next starts when this one ends.
            launcher.0.lock().expect("not poisoned").exited[index] = Some(Some(0));
            queue.tick(Instant::now());
        }
        let endpoints: Vec<_> = (0..planned.len())
            .map(|index| spec_of(&launcher, index)["ai"]["endpoint"].clone())
            .collect();
        assert_eq!(
            endpoints,
            [
                "http://127.0.0.1:40123",
                "http://localhost:11434",
                "http://127.0.0.1:1234/v1"
            ],
            "the built-in server, Ollama, the custom endpoint"
        );
    }

    /// With a library folder, every book is saved there under its PDF's name — never over a book
    /// already there, nor over one a waiting job will write.
    #[test]
    fn books_are_saved_in_the_folder_given() {
        let (mut queue, _, _, dir) = recording("folder");
        let library = dir.join("OpenConvert");
        std::fs::create_dir_all(&library).expect("made");
        std::fs::write(library.join("Book.epub"), "x").expect("an earlier book");
        queue.enqueue_into(
            &[
                dir.join("a").join("Book.pdf"),
                dir.join("b").join("Book.pdf"),
            ],
            PresetName::Auto,
            None,
            &JobAi::Off,
            Some(&library),
        );
        let views = queue.views();
        assert_eq!(views[0].output, library.join("Book (2).epub"));
        assert!(views[0].renamed);
        assert_eq!(views[1].output, library.join("Book (3).epub"));

        queue.enqueue_into(
            &[dir.join("c.pdf")],
            PresetName::Auto,
            None,
            &JobAi::Off,
            None,
        );
        assert_eq!(
            queue.views()[2].output,
            dir.join("c.epub"),
            "no folder: beside the PDF"
        );
    }

    /// Each job that ends is reported once — the history's record of it — with when it started,
    /// how it ended, and whether the user cancelled it.
    #[test]
    fn a_finished_job_is_reported_once() {
        let (mut queue, launcher, finishes, dir) = recording("finished");
        let before = std::time::SystemTime::now();
        let ids = queue.enqueue_into(
            &[dir.join("a.pdf"), dir.join("b.pdf"), dir.join("c.pdf")],
            PresetName::Auto,
            None,
            &JobAi::Off,
            None,
        );
        assert!(finishes.0.lock().expect("not poisoned").is_empty());

        launcher.0.lock().expect("not poisoned").exited[0] = Some(Some(0));
        queue.tick(Instant::now());
        queue.tick(Instant::now());
        let first = finishes.0.lock().expect("not poisoned").clone();
        assert_eq!(first.len(), 1, "once, however often the clock ticks");
        assert_eq!(first[0].id, ids[0]);
        assert_eq!(first[0].state, JobState::Exited { code: Some(0) });
        assert_eq!(first[0].output, dir.join("a.epub"));
        let mut report = dir.join("a.epub").into_os_string();
        report.push(".report.json");
        assert_eq!(first[0].report, PathBuf::from(report));
        assert!(!first[0].cancelled);
        let started = first[0].started.expect("its engine started");
        assert!(started >= before && first[0].ended >= started);

        // Cancelled while running, and before it ever started: both reported, both marked.
        queue.cancel(&ids[2], Instant::now()).expect("known");
        queue.cancel(&ids[1], Instant::now()).expect("known");
        launcher.0.lock().expect("not poisoned").exited[1] = Some(Some(3));
        queue.tick(Instant::now());
        let all = finishes.0.lock().expect("not poisoned").clone();
        assert_eq!(all.len(), 3);
        let waiting = all.iter().find(|job| job.id == ids[2]).expect("reported");
        assert_eq!(waiting.state, JobState::CancelledBeforeStart);
        assert!(waiting.cancelled && waiting.started.is_none());
        let running = all.iter().find(|job| job.id == ids[1]).expect("reported");
        assert!(running.cancelled);
    }
}
