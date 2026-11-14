//! Background preview rendering.
//!
//! Dragging sliders used to render the patch/music cache synchronously on the
//! UI thread. A single worker thread now renders the newest requested state;
//! stale jobs are skipped through the shared `latest` sequence and stale
//! results are dropped before they touch the cache.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::model::{Music, Patch};
use crate::synth::{self, Envelope};

use super::*;

/// Identity of the document state a preview belongs to. Results are applied
/// only when this still matches the current editor state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct RenderKey {
    pub tab: Tab,
    pub rate: u32,
    pub version: u64,
    pub music_version: u64,
    /// Stable patch id (0 only if the project is empty).
    pub patch: u32,
}

enum RenderPayload {
    Patch(Patch),
    Music(Music),
}

struct RenderJob {
    seq: u64,
    key: RenderKey,
    payload: RenderPayload,
}

pub(super) struct RenderOutcome {
    pub seq: u64,
    pub key: RenderKey,
    pub samples: Vec<f32>,
    pub envelope: Envelope,
    pub peak: f32,
}

/// Owner-side handle for the render thread. Dropping it closes the job
/// channel and lets the worker exit.
pub(super) struct RenderWorker {
    jobs: Sender<RenderJob>,
    results: Receiver<RenderOutcome>,
    latest: Arc<AtomicU64>,
    seq: u64,
    pending: Option<(u64, RenderKey)>,
    pending_since: Instant,
    last_request: Instant,
}

impl RenderWorker {
    pub(super) fn spawn() -> Option<Self> {
        let (jobs, jobs_rx) = mpsc::channel::<RenderJob>();
        let (results_tx, results) = mpsc::channel::<RenderOutcome>();
        let latest = Arc::new(AtomicU64::new(0));
        let worker_latest = latest.clone();
        thread::Builder::new()
            .name("sfx-render".into())
            .spawn(move || worker_loop(jobs_rx, results_tx, worker_latest))
            .ok()?;
        Some(Self {
            jobs,
            results,
            latest,
            seq: 0,
            pending: None,
            pending_since: Instant::now(),
            last_request: Instant::now() - Duration::from_secs(1),
        })
    }

    fn request(&mut self, key: RenderKey, payload: RenderPayload) -> Option<u64> {
        if self.pending_key() == Some(key) {
            return None;
        }
        self.seq = self.seq.wrapping_add(1);
        self.latest.store(self.seq, Ordering::Release);
        self.pending = Some((self.seq, key));
        self.pending_since = Instant::now();
        self.last_request = Instant::now();
        let job = RenderJob {
            seq: self.seq,
            key,
            payload,
        };
        self.jobs.send(job).ok()?;
        Some(self.seq)
    }

    fn try_result(&mut self) -> Option<RenderOutcome> {
        self.results.try_recv().ok()
    }

    fn finish(&mut self, seq: u64) {
        if self.pending.map(|(pending, _)| pending) == Some(seq) {
            self.pending = None;
        }
    }

    fn pending_key(&self) -> Option<RenderKey> {
        self.pending.map(|(_, key)| key)
    }

    fn cancel(&mut self) {
        self.seq = self.seq.wrapping_add(1);
        self.latest.store(self.seq, Ordering::Release);
        self.pending = None;
    }

    fn throttle_ok(&self) -> bool {
        self.last_request.elapsed() >= Duration::from_millis(80)
    }

    fn stalled(&self) -> bool {
        self.pending.is_some() && self.pending_since.elapsed() > Duration::from_secs(5)
    }
}

fn worker_loop(jobs: Receiver<RenderJob>, results: Sender<RenderOutcome>, latest: Arc<AtomicU64>) {
    for job in jobs {
        if latest.load(Ordering::Acquire) != job.seq {
            continue;
        }
        let samples = match job.payload {
            RenderPayload::Patch(patch) => synth::render_patch(&patch, job.key.rate),
            RenderPayload::Music(music) => synth::render_music(&music, job.key.rate),
        };
        if latest.load(Ordering::Acquire) != job.seq {
            continue;
        }
        let envelope = synth::envelope(&samples, synth::ENVELOPE_BINS);
        let peak = synth::peak(&samples);
        if results
            .send(RenderOutcome {
                seq: job.seq,
                key: job.key,
                samples,
                envelope,
                peak,
            })
            .is_err()
        {
            break;
        }
    }
}

impl SfxLogic {
    /// Drain finished jobs and apply the newest result that still matches the
    /// current document state.
    pub(super) fn poll_render(&mut self) {
        let mut outcomes = Vec::new();
        if let Some(worker) = self.render.as_mut() {
            while let Some(outcome) = worker.try_result() {
                outcomes.push(outcome);
            }
        }
        for outcome in outcomes {
            let current = self.preview_key(outcome.key.rate);
            let pending = self.render.as_ref().and_then(|worker| worker.pending_key());
            if pending != Some(outcome.key) || outcome.key != current {
                continue;
            }
            if let Some(worker) = self.render.as_mut() {
                worker.finish(outcome.seq);
            }
            self.apply_render(outcome);
            break;
        }
    }

    /// Queue a preview render if the cache is stale and the throttle allows.
    pub(super) fn request_preview(&mut self, rate: u32) {
        if !self.state.preview_pending() {
            return;
        }
        let key = self.preview_key(rate);
        let payload = match key.tab {
            Tab::Patches => RenderPayload::Patch(self.state.patch().cloned().unwrap_or_default()),
            Tab::Music => RenderPayload::Music(self.state.doc.project.music.clone()),
        };
        let Some(worker) = self.render.as_mut() else {
            self.state.ensure_cache_soon(rate);
            return;
        };
        if worker.stalled() {
            worker.cancel();
            self.state.ensure_cache_soon(rate);
            return;
        }
        if !worker.throttle_ok() {
            return;
        }
        worker.request(key, payload);
    }

    /// Drop any in-flight preview (used before a synchronous render such as
    /// pressing play, so a stale result cannot overwrite fresh samples).
    pub(super) fn cancel_preview(&mut self) {
        if let Some(worker) = self.render.as_mut() {
            worker.cancel();
        }
    }

    fn preview_key(&self, rate: u32) -> RenderKey {
        RenderKey {
            tab: self.state.view.tab,
            rate,
            version: self.state.doc.version,
            music_version: self.state.doc.music_version,
            patch: self.state.patch().map(|patch| patch.id).unwrap_or(0),
        }
    }

    fn apply_render(&mut self, outcome: RenderOutcome) {
        match outcome.key.tab {
            Tab::Patches => {
                self.state.playback.cache = outcome.samples;
                self.state.playback.cache_peak = outcome.peak;
                self.state.playback.cache_envelope = outcome.envelope;
                self.state.playback.cache_version = outcome.key.version;
                self.state.playback.cache_rate = outcome.key.rate;
            }
            Tab::Music => {
                self.state.playback.music_cache = outcome.samples;
                self.state.playback.music_cache_peak = outcome.peak;
                self.state.playback.music_envelope = outcome.envelope;
                self.state.playback.music_cache_version = outcome.key.music_version;
                self.state.playback.music_cache_rate = outcome.key.rate;
            }
        }
        self.state.playback.last_render = Instant::now();
        self.state.bump_view();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Layer, Wave};

    fn key(version: u64) -> RenderKey {
        RenderKey {
            tab: Tab::Patches,
            rate: 8000,
            version,
            music_version: 1,
            patch: 0,
        }
    }

    fn short_patch() -> Patch {
        let mut layer = Layer::new(Wave::Sine);
        layer.duration = 0.02;
        Patch {
            layers: vec![layer],
            ..Patch::default()
        }
    }

    #[test]
    fn identical_keys_are_not_requested_twice() {
        let Some(mut worker) = RenderWorker::spawn() else {
            return;
        };
        let first = key(1);
        assert!(worker
            .request(first, RenderPayload::Patch(short_patch()))
            .is_some());
        assert!(worker
            .request(first, RenderPayload::Patch(short_patch()))
            .is_none());
        assert!(worker
            .request(key(2), RenderPayload::Patch(short_patch()))
            .is_some());
    }

    #[test]
    fn worker_renders_the_requested_patch() {
        let Some(mut worker) = RenderWorker::spawn() else {
            return;
        };
        let request = key(7);
        let seq = worker
            .request(request, RenderPayload::Patch(short_patch()))
            .expect("job queued");
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Some(outcome) = worker.try_result() {
                assert_eq!(outcome.seq, seq);
                assert_eq!(outcome.key, request);
                assert!(!outcome.samples.is_empty());
                assert_eq!(outcome.envelope.min.len(), synth::ENVELOPE_BINS);
                assert!(outcome.peak > 0.0);
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("render worker did not respond in time");
    }
}
