use crate::{
    model::{now, DownloadOptions, Job, Progress, Settings},
    process::WorkerFailure,
    store::Store,
};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::sync::{watch, Semaphore};

pub struct Running {
    pub attempt: String,
    pub engine: String,
    pub cancel: watch::Sender<bool>,
}

pub struct Queue {
    pub store: Store,
    pub jobs: Vec<Job>,
    pub settings: Settings,
    pub running: HashMap<String, Running>,
    pub metadata_slots: Arc<Semaphore>,
    pub mux_slots: Arc<Semaphore>,
    pub stopping: bool,
}

impl Queue {
    pub fn new(store: Store) -> Result<Self, String> {
        let settings = store.settings()?;
        settings.validate()?;
        let jobs = store.jobs()?;
        Ok(Self {
            store,
            jobs,
            settings,
            running: HashMap::new(),
            metadata_slots: Arc::new(Semaphore::new(2)),
            mux_slots: Arc::new(Semaphore::new(1)),
            stopping: false,
        })
    }

    pub fn settings(&mut self, settings: Settings) -> Result<(), String> {
        self.store.save_settings(&settings)?;
        self.settings = settings;
        Ok(())
    }

    pub fn enqueue(
        &mut self,
        media: &[Value],
        options: DownloadOptions,
    ) -> Result<Vec<String>, String> {
        options.validate()?;
        let mut created = Vec::new();
        let mut position = self.jobs.iter().map(|j| j.position).max().unwrap_or(0);
        for item in media {
            let url = item["url"].as_str().ok_or("URL mancante.")?;
            let title = item["title"].as_str().unwrap_or("Video senza titolo");
            position += 1;
            created.push(Job {
                id: uuid::Uuid::new_v4().to_string(),
                url: url.into(),
                title: title.into(),
                channel: item["channel"]
                    .as_str()
                    .unwrap_or("Canale non disponibile")
                    .into(),
                thumbnail: item["thumbnail"].as_str().map(String::from),
                filename: item["filename"].as_str().unwrap_or(title).into(),
                options: options.clone(),
                state: "queued".into(),
                position,
                progress: Progress::default(),
                selection: None,
                output: None,
                temporary: None,
                engine: String::new(),
                attempt_id: String::new(),
                retries: 0,
                recovery_retried: false,
                error: None,
                updated_at: now(),
                ready_at: 0,
            });
        }
        self.store.save_jobs(&created)?;
        let ids = created.iter().map(|j| j.id.clone()).collect();
        self.jobs.extend(created);
        Ok(ids)
    }

    pub fn reorder(&mut self, ids: &[String]) -> Result<(), String> {
        let waiting: HashSet<_> = self
            .jobs
            .iter()
            .filter(|j| j.state == "queued")
            .map(|j| j.id.clone())
            .collect();
        if ids.len() != waiting.len()
            || ids.iter().collect::<HashSet<_>>().len() != ids.len()
            || ids.iter().any(|id| !waiting.contains(id))
        {
            return Err("Riordina tutti e soli gli elementi in attesa.".into());
        }
        let mut positions: Vec<_> = self
            .jobs
            .iter()
            .filter(|j| j.state == "queued")
            .map(|j| j.position)
            .collect();
        positions.sort();
        let mut changed = self.jobs.clone();
        for (id, position) in ids.iter().zip(positions) {
            changed.iter_mut().find(|j| &j.id == id).unwrap().position = position;
        }
        self.store.save_jobs(&changed)?;
        self.jobs = changed;
        self.jobs.sort_by_key(|j| j.position);
        Ok(())
    }

    pub fn claim(&mut self, engine: &str) -> Result<Vec<(Job, watch::Receiver<bool>)>, String> {
        if self.stopping {
            return Ok(Vec::new());
        }
        let available = self
            .settings
            .concurrent_jobs
            .saturating_sub(self.running.len());
        let mut candidates: Vec<_> = self
            .jobs
            .iter()
            .filter(|j| {
                j.state == "queued" && j.ready_at <= now() && !self.running.contains_key(&j.id)
            })
            .cloned()
            .collect();
        candidates.sort_by_key(|j| j.position);
        let mut result = Vec::new();
        for mut job in candidates.into_iter().take(available) {
            job.state = "resolving".into();
            if job.engine.is_empty() {
                job.engine = engine.into();
            }
            job.attempt_id = uuid::Uuid::new_v4().to_string();
            job.updated_at = now();
            job.error = None;
            self.store.save_job(&job)?;
            self.store.record_attempt(
                &job.attempt_id,
                &job.id,
                &serde_json::json!({"engine":engine,"started":now()}),
            )?;
            let (tx, rx) = watch::channel(false);
            self.running.insert(
                job.id.clone(),
                Running {
                    attempt: job.attempt_id.clone(),
                    engine: job.engine.clone(),
                    cancel: tx,
                },
            );
            *self.jobs.iter_mut().find(|j| j.id == job.id).unwrap() = job.clone();
            result.push((job, rx));
        }
        Ok(result)
    }

    pub fn control(&mut self, id: &str, action: &str) -> Result<(), String> {
        let index = self
            .jobs
            .iter()
            .position(|j| j.id == id)
            .ok_or("Download non trovato.")?;
        let mut job = self.jobs[index].clone();
        match action {
            "pause" if job.active() || job.state == "queued" => {
                job.state = "paused".into();
                job.progress.speed = None;
                job.progress.eta = None;
            }
            "resume" if job.state == "paused" => {
                job.state = "queued".into();
                job.ready_at = 0;
            }
            "retry" if ["failed", "cancelled"].contains(&job.state.as_str()) => {
                if self.running.contains_key(id) {
                    return Err("Attendi l'arresto del tentativo precedente.".into());
                }
                let current = self
                    .store
                    .get::<String>("active_engine")?
                    .unwrap_or_else(|| "baseline".into());
                if job.engine != current {
                    job.engine = current;
                    job.temporary = None;
                    job.output = None;
                    job.selection = None;
                    job.progress = Default::default();
                }
                job.state = "queued".into();
                job.retries = 0;
                job.ready_at = 0;
                job.error = None;
            }
            "cancel" if !job.terminal() => {
                job.state = "cancelled".into();
                job.progress.speed = None;
                job.progress.eta = None;
            }
            "remove" if job.terminal() && !self.running.contains_key(id) => {
                self.store.remove_job(id)?;
                self.jobs.remove(index);
                return Ok(());
            }
            _ => return Err("Azione non disponibile per questo stato.".into()),
        }
        job.updated_at = now();
        self.store.save_job(&job)?;
        if ["pause", "cancel"].contains(&action) {
            if let Some(running) = self.running.get(id) {
                let _ = running.cancel.send(true);
            }
        }
        self.jobs[index] = job;
        Ok(())
    }

    pub fn phase(&mut self, id: &str, attempt: &str, state: &str) -> Result<bool, String> {
        if !["resolving", "downloading", "muxing"].contains(&state) {
            return Err("Fase non valida.".into());
        }
        if let Some(job) = self
            .jobs
            .iter_mut()
            .find(|j| j.id == id && j.attempt_id == attempt && j.active())
        {
            job.state = state.into();
            job.updated_at = now();
            if state == "muxing" {
                job.progress.speed = None;
                job.progress.eta = None;
                job.progress.detail = "Unione e verifica delle tracce".into();
            }
            self.store.save_job(job)?;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn progress(&mut self, id: &str, attempt: &str, progress: Progress) {
        if let Some(job) = self
            .jobs
            .iter_mut()
            .find(|j| j.id == id && j.attempt_id == attempt && j.active())
        {
            job.progress = progress;
        }
    }

    pub fn paths(
        &mut self,
        id: &str,
        attempt: &str,
        temporary: &str,
        output: &str,
    ) -> Result<(), String> {
        let job = self
            .jobs
            .iter_mut()
            .find(|j| j.id == id && j.attempt_id == attempt && j.active())
            .ok_or("Download interrotto.")?;
        job.temporary = Some(temporary.into());
        job.output = Some(output.into());
        self.store.save_job(job)
    }

    pub fn finish(
        &mut self,
        id: &str,
        attempt: &str,
        result: Result<String, WorkerFailure>,
    ) -> Result<(), String> {
        if self.running.get(id).is_none_or(|r| r.attempt != attempt) {
            return Ok(());
        }
        self.running.remove(id);
        let Some(job) = self
            .jobs
            .iter_mut()
            .find(|j| j.id == id && j.attempt_id == attempt)
        else {
            return Ok(());
        };
        // Pause/cancel (or a resume queued while stopping) owns the state transition.
        if !job.active() {
            return Ok(());
        }
        job.updated_at = now();
        job.progress.speed = None;
        job.progress.eta = None;
        match result {
            Ok(output) => {
                job.state = "completed".into();
                job.output = Some(output);
                job.error = None;
            }
            Err(error) if error.retryable && job.retries < 3 => {
                job.retries += 1;
                job.state = "queued".into();
                let delay = error
                    .retry_after
                    .unwrap_or(2_u64.pow(job.retries + 1) + (job.id.as_bytes()[0] % 3) as u64);
                job.ready_at = now().saturating_add(delay);
                job.error = Some(error);
            }
            Err(error) => {
                job.state = "failed".into();
                job.error = Some(error);
            }
        }
        self.store.save_job(job)
    }

    pub fn recover_interrupted(&mut self) -> Result<(), String> {
        let mut changed = self.jobs.clone();
        for job in &mut changed {
            if job.active() {
                job.state = "paused".into();
                job.progress.speed = None;
                job.progress.eta = None;
                job.progress.detail =
                    "Download interrotto. Riprendi per continuare, se la sorgente lo consente."
                        .into();
            }
        }
        self.store.save_jobs(&changed)?;
        self.jobs = changed;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), String> {
        self.stopping = true;
        let ids: Vec<_> = self
            .jobs
            .iter()
            .filter(|j| j.active() || j.state == "queued")
            .map(|j| j.id.clone())
            .collect();
        for id in ids {
            self.control(&id, "pause")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn queue() -> (tempfile::TempDir, Queue) {
        let temp = tempfile::tempdir().unwrap();
        let queue = Queue::new(Store::open(temp.path()).unwrap()).unwrap();
        (temp, queue)
    }
    fn media(count: usize) -> Vec<Value> {
        (0..count).map(|i| serde_json::json!({"url":format!("https://www.youtube.com/watch?v=fixture{i:04}"),"title":format!("Video {i}")})).collect()
    }

    #[tokio::test]
    async fn concurrency_and_reduction_keep_existing_jobs() {
        let (_temp, mut queue) = queue();
        queue
            .enqueue(&media(10), DownloadOptions::default())
            .unwrap();
        let claimed = queue.claim("baseline").unwrap();
        assert_eq!(claimed.len(), 3);
        queue.settings.concurrent_jobs = 1;
        assert!(queue.claim("baseline").unwrap().is_empty());
        queue
            .finish(
                &claimed[0].0.id,
                &claimed[0].0.attempt_id,
                Ok("one.mp4".into()),
            )
            .unwrap();
        queue
            .finish(
                &claimed[1].0.id,
                &claimed[1].0.attempt_id,
                Ok("two.mp4".into()),
            )
            .unwrap();
        assert!(queue.claim("baseline").unwrap().is_empty());
        queue
            .finish(
                &claimed[2].0.id,
                &claimed[2].0.attempt_id,
                Ok("three.mp4".into()),
            )
            .unwrap();
        assert_eq!(queue.claim("baseline").unwrap().len(), 1);
        let a = queue.metadata_slots.clone().acquire_owned().await.unwrap();
        let b = queue.metadata_slots.clone().acquire_owned().await.unwrap();
        assert!(queue.metadata_slots.clone().try_acquire_owned().is_err());
        drop((a, b));
        let _mux = queue.mux_slots.clone().acquire_owned().await.unwrap();
        assert!(queue.mux_slots.clone().try_acquire_owned().is_err());
    }

    #[test]
    fn settings_snapshot_and_order_survive_reopen() {
        let (temp, mut queue) = queue();
        let mut options = DownloadOptions::default();
        options.destination = "C:\\first".into();
        let ids = queue.enqueue(&media(3), options.clone()).unwrap();
        let reversed: Vec<_> = ids.iter().rev().cloned().collect();
        queue.reorder(&reversed).unwrap();
        let mut settings = queue.settings.clone();
        settings.defaults.destination = "C:\\changed".into();
        queue.settings(settings).unwrap();
        drop(queue);
        let queue = Queue::new(Store::open(temp.path()).unwrap()).unwrap();
        assert_eq!(
            queue.jobs.iter().map(|j| j.id.clone()).collect::<Vec<_>>(),
            reversed
        );
        assert!(queue.jobs.iter().all(|j| j.options == options));
        assert_eq!(queue.settings.defaults.destination, "C:\\changed");
    }

    #[test]
    fn pause_waits_for_worker_exit_before_slot_reuse() {
        let (_temp, mut queue) = queue();
        queue.settings.concurrent_jobs = 1;
        queue
            .enqueue(&media(2), DownloadOptions::default())
            .unwrap();
        let claimed = queue.claim("baseline").unwrap();
        let (job, cancel) = &claimed[0];
        queue.control(&job.id, "pause").unwrap();
        assert!(*cancel.borrow());
        assert!(queue.claim("baseline").unwrap().is_empty());
        queue
            .finish(
                &job.id,
                &job.attempt_id,
                Err(WorkerFailure::new("cancelled", "stop")),
            )
            .unwrap();
        assert_eq!(queue.jobs[0].state, "paused");
        assert_eq!(queue.claim("baseline").unwrap().len(), 1);
    }

    #[test]
    fn retry_budget_and_crash_recovery() {
        let (_temp, mut queue) = queue();
        queue
            .enqueue(&media(1), DownloadOptions::default())
            .unwrap();
        for attempt in 0..4 {
            queue.jobs[0].ready_at = 0;
            let claimed = queue.claim("baseline").unwrap();
            let job = &claimed[0].0;
            let mut error = WorkerFailure::new("network", "rate limit");
            error.retryable = true;
            error.retry_after = Some(30);
            queue.finish(&job.id, &job.attempt_id, Err(error)).unwrap();
            assert_eq!(
                queue.jobs[0].state,
                if attempt < 3 { "queued" } else { "failed" }
            );
            if attempt < 3 {
                assert!(queue.jobs[0].ready_at >= now() + 29);
            }
        }
        let id = queue.jobs[0].id.clone();
        queue
            .store
            .set("active_engine", &"updated-environment")
            .unwrap();
        queue.jobs[0].temporary = Some("old-partials".into());
        queue.control(&id, "retry").unwrap();
        assert_eq!(queue.jobs[0].engine, "updated-environment");
        assert!(queue.jobs[0].temporary.is_none());
        queue.claim("baseline").unwrap();
        queue.recover_interrupted().unwrap();
        assert_eq!(queue.jobs[0].state, "paused");
    }
}
