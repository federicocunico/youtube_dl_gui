use crate::{
    files,
    model::{now, Job, Progress},
    process::{run_worker, RuntimePaths, WorkerFailure},
    queue::Queue,
    store::Store,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{watch, Notify};

pub struct Service {
    pub queue: Mutex<Queue>,
    pub resources: PathBuf,
    pub data: PathBuf,
    pub notify: Notify,
    pub discoveries: Mutex<HashMap<String, watch::Sender<bool>>>,
    pub update: Mutex<Value>,
    pub update_lock: tokio::sync::Mutex<()>,
    pub emit: Box<dyn Fn(&str, Value) + Send + Sync>,
}

impl Service {
    pub fn new(
        resources: PathBuf,
        data: PathBuf,
        downloads: PathBuf,
        emit: impl Fn(&str, Value) + Send + Sync + 'static,
    ) -> Result<Arc<Self>, String> {
        let mut queue = Queue::new(Store::open(&data)?)?;
        if queue.settings.defaults.destination.is_empty() {
            let mut settings = queue.settings.clone();
            settings.defaults.destination = downloads.to_string_lossy().into();
            queue.settings(settings)?;
        }
        queue.recover_interrupted()?;
        if queue.store.get::<String>("active_engine")?.is_none() {
            queue.store.set("active_engine", &"baseline")?;
        }
        let update = crate::updater::recover_startup(&mut queue, &data)?;
        let service = Arc::new(Self {
            queue: Mutex::new(queue),
            resources,
            data,
            notify: Notify::new(),
            discoveries: Mutex::new(HashMap::new()),
            update: Mutex::new(update),
            update_lock: tokio::sync::Mutex::new(()),
            emit: Box::new(emit),
        });
        if let Ok(version) = crate::updater::version(&service, &service.active()) {
            service.update.lock().unwrap()["installed_version"] = json!(version);
        }
        Ok(service)
    }

    pub fn active(&self) -> String {
        self.queue
            .lock()
            .unwrap()
            .store
            .get::<String>("active_engine")
            .ok()
            .flatten()
            .unwrap_or_else(|| "baseline".into())
    }

    pub fn runtime(&self, engine: &str) -> Result<RuntimePaths, String> {
        let mut runtime = RuntimePaths::bundled(&self.resources)?;
        if engine != "baseline" {
            if !uuid::Uuid::parse_str(engine).is_ok() {
                return Err("Ambiente motore non valido.".into());
            }
            runtime.python = self
                .data
                .join("engines")
                .join(engine)
                .join("python/python.exe");
        }
        runtime.verify()?;
        Ok(runtime)
    }

    pub fn snapshot(&self) -> Result<Value, String> {
        let queue = self.queue.lock().unwrap();
        Ok(
            json!({"jobs":queue.jobs,"settings":queue.settings,"active_engine":queue.store.get::<String>("active_engine")?,"previous_engine":queue.store.get::<String>("previous_engine")?,"history":queue.store.history()?,"update":*self.update.lock().unwrap()}),
        )
    }

    pub fn changed(&self) {
        if let Ok(value) = self.snapshot() {
            (self.emit)("snapshot", value);
        }
    }

    pub fn start(self: &Arc<Self>) {
        let service = self.clone();
        tauri::async_runtime::spawn(async move {
            service.reconcile().await;
            loop {
                let active = service.active();
                let claimed = service.queue.lock().unwrap().claim(&active);
                match claimed {
                    Ok(jobs) => {
                        for (job, cancel) in jobs {
                            let worker = service.clone();
                            tauri::async_runtime::spawn(async move {
                                let result = worker.download_job(&job, cancel).await;
                                let failure = result.as_ref().err().cloned();
                                if failure.as_ref().is_some_and(|e| {
                                    ["engine", "protocol"].contains(&e.code.as_str())
                                }) {
                                    if let Err(error) =
                                        crate::updater::recover(worker.clone(), job.engine.clone())
                                            .await
                                    {
                                        (worker.emit)("app-error", json!(error));
                                    }
                                }
                                let outcome = worker.queue.lock().unwrap().finish(
                                    &job.id,
                                    &job.attempt_id,
                                    result,
                                );
                                if let Err(error) = outcome {
                                    (worker.emit)("app-error", json!(error));
                                }
                                let completed = worker
                                    .queue
                                    .lock()
                                    .unwrap()
                                    .jobs
                                    .iter()
                                    .find(|j| j.id == job.id && j.state == "completed")
                                    .cloned();
                                if let Some(completed) = completed {
                                    if let Some(temp) = completed.temporary {
                                        let _ = files::clean_temporary(
                                            Path::new(&temp),
                                            Path::new(&completed.options.destination),
                                            &completed.id,
                                        );
                                    }
                                }
                                worker.changed();
                                worker.notify.notify_one();
                                if let Some(error) = failure {
                                    if ["engine", "protocol"].contains(&error.code.as_str()) {
                                        (worker.emit)(
                                            "engine-failure",
                                            json!({"engine":job.engine,"job":job.id,"error":error}),
                                        );
                                    }
                                }
                            });
                        }
                    }
                    Err(error) => (service.emit)("app-error", json!(error)),
                }
                service.changed();
                tokio::select! { _=service.notify.notified()=>{}, _=tokio::time::sleep(Duration::from_secs(1))=>{} }
                if service.queue.lock().unwrap().stopping {
                    break;
                }
            }
        });
    }

    async fn reconcile(&self) {
        let jobs = self.queue.lock().unwrap().jobs.clone();
        for job in jobs
            .into_iter()
            .filter(|j| j.state == "paused" && j.output.is_some())
        {
            let path = PathBuf::from(job.output.as_ref().unwrap());
            if !path.is_file() {
                continue;
            }
            let Some(temp) = job.temporary.as_ref() else {
                continue;
            };
            let journal = std::fs::read_to_string(Path::new(temp).join("completion.json"))
                .ok()
                .and_then(|v| serde_json::from_str::<Value>(&v).ok());
            if !journal.is_some_and(|v| v["output"] == json!(path) && v["verified"] == true) {
                continue;
            }
            let Ok(runtime) = self.runtime(&job.engine) else {
                continue;
            };
            let (_tx, cancel) = watch::channel(false);
            let verified = run_worker(
                runtime,
                job.id.clone(),
                uuid::Uuid::new_v4().to_string(),
                "download",
                json!({"phase":"verify","file":path,"audio_only":job.options.audio_only}),
                cancel,
                Duration::from_secs(40),
                |_| {},
            )
            .await;
            if verified.is_ok() {
                let mut queue = self.queue.lock().unwrap();
                if let Some(current) = queue.jobs.iter_mut().find(|j| j.id == job.id) {
                    current.state = "completed".into();
                    current.error = None;
                    current.updated_at = now();
                    let saved = current.clone();
                    let _ = queue.store.save_job(&saved);
                }
            }
        }
        self.changed();
    }

    pub(crate) async fn download_job(
        self: &Arc<Self>,
        job: &Job,
        cancel: watch::Receiver<bool>,
    ) -> Result<String, WorkerFailure> {
        let runtime = self
            .runtime(&job.engine)
            .map_err(|e| WorkerFailure::new("runtime", &e))?;
        let destination = files::destination(&job.options.destination)
            .map_err(|e| WorkerFailure::new("destination", &e))?;
        let directory = if let Some(previous) = &job.temporary {
            let path = PathBuf::from(previous);
            if path.is_dir() && path.parent().is_some_and(|p| p == destination) {
                path
            } else {
                destination.join(format!(".frame-{}-{}", job.id, job.engine))
            }
        } else {
            destination.join(format!(".frame-{}-{}", job.id, job.engine))
        };
        std::fs::create_dir_all(&directory).map_err(|_| {
            WorkerFailure::new("destination", "Impossibile creare i file temporanei.")
        })?;
        let payload = json!({"url":job.url,"options":job.options,"directory":directory});
        let metadata = self.queue.lock().unwrap().metadata_slots.clone();
        let _permit = tokio::select! {p=metadata.acquire_owned()=>p.map_err(|_|WorkerFailure::new("cancelled","Operazione interrotta."))?,_=wait_cancel(cancel.clone())=>return Err(WorkerFailure::new("cancelled","Operazione annullata."))};
        // Inspect refreshes expiring media URLs and validates the actual format before transfer.
        let selection = run_worker(
            runtime.clone(),
            job.id.clone(),
            job.attempt_id.clone(),
            "inspect",
            payload.clone(),
            cancel.clone(),
            Duration::from_secs(90),
            |_| {},
        )
        .await?;
        drop(_permit);
        {
            let mut queue = self.queue.lock().unwrap();
            if let Some(current) = queue
                .jobs
                .iter_mut()
                .find(|j| j.id == job.id && j.attempt_id == job.attempt_id)
            {
                current.selection = Some(selection["selection"].clone());
                let current = current.clone();
                queue
                    .store
                    .save_job(&current)
                    .map_err(|e| WorkerFailure::new("runtime", &e))?;
            }
        }
        let extension = selection["selection"]["container"]
            .as_str()
            .ok_or_else(|| WorkerFailure::new("protocol", "Contenitore mancante."))?;
        let proposed = files::candidate(&destination, &job.filename, extension, 0)
            .map_err(|e| WorkerFailure::new("destination", &e))?;
        self.queue
            .lock()
            .unwrap()
            .paths(
                &job.id,
                &job.attempt_id,
                &directory.to_string_lossy(),
                &proposed.to_string_lossy(),
            )
            .map_err(|e| WorkerFailure::new("cancelled", &e))?;
        self.queue
            .lock()
            .unwrap()
            .phase(&job.id, &job.attempt_id, "downloading")
            .map_err(|e| WorkerFailure::new("runtime", &e))?;
        let service = self.clone();
        let id = job.id.clone();
        let attempt = job.attempt_id.clone();
        let metadata = self.queue.lock().unwrap().metadata_slots.clone();
        let permit = tokio::select! {p=metadata.acquire_owned()=>p.map_err(|_|WorkerFailure::new("cancelled","Operazione interrotta."))?,_=wait_cancel(cancel.clone())=>return Err(WorkerFailure::new("cancelled","Operazione annullata."))};
        let metadata_guard = Arc::new(Mutex::new(Some(permit)));
        let guard = metadata_guard.clone();
        let transferred = run_worker(
            runtime.clone(),
            job.id.clone(),
            job.attempt_id.clone(),
            "download",
            payload.clone(),
            cancel.clone(),
            Duration::from_secs(24 * 3600),
            move |e| {
                if e["event"] == "phase" && e["data"]["state"] == "transfer_ready" {
                    guard.lock().unwrap().take();
                }
                if e["event"] == "progress" {
                    if let Ok(progress) = serde_json::from_value::<Progress>(e["data"].clone()) {
                        service
                            .queue
                            .lock()
                            .unwrap()
                            .progress(&id, &attempt, progress);
                        service.changed();
                    }
                }
            },
        )
        .await?;
        drop(metadata_guard);
        let slots = self.queue.lock().unwrap().mux_slots.clone();
        let _permit = tokio::select! {p=slots.acquire_owned()=>p.map_err(|_|WorkerFailure::new("cancelled","Operazione interrotta."))?,_=wait_cancel(cancel.clone())=>return Err(WorkerFailure::new("cancelled","Operazione annullata."))};
        self.queue
            .lock()
            .unwrap()
            .phase(&job.id, &job.attempt_id, "muxing")
            .map_err(|e| WorkerFailure::new("runtime", &e))?;
        self.changed();
        let mut payload = payload;
        payload["phase"] = json!("finalize");
        payload["files"] = transferred["files"].clone();
        payload["selection"] = transferred["selection"].clone();
        let verified = run_worker(
            runtime,
            job.id.clone(),
            job.attempt_id.clone(),
            "download",
            payload,
            cancel.clone(),
            Duration::from_secs(1900),
            |_| {},
        )
        .await?;
        let verified_path = PathBuf::from(
            verified["verified_path"]
                .as_str()
                .ok_or_else(|| WorkerFailure::new("protocol", "File finale mancante."))?,
        );
        if verified_path.parent() != Some(directory.as_path()) {
            return Err(WorkerFailure::new(
                "protocol",
                "File finale fuori dalla directory temporanea.",
            ));
        }
        let extension = verified["selection"]["container"]
            .as_str()
            .ok_or_else(|| WorkerFailure::new("protocol", "Contenitore finale mancante."))?;
        for suffix in 0..10000 {
            if *cancel.borrow() {
                return Err(WorkerFailure::new("cancelled", "Operazione annullata."));
            }
            let target = files::candidate(&destination, &job.filename, extension, suffix)
                .map_err(|e| WorkerFailure::new("destination", &e))?;
            if target.exists() {
                continue;
            }
            self.queue
                .lock()
                .unwrap()
                .paths(
                    &job.id,
                    &job.attempt_id,
                    &directory.to_string_lossy(),
                    &target.to_string_lossy(),
                )
                .map_err(|e| WorkerFailure::new("cancelled", &e))?;
            let journal = json!({"output":target,"verified":true});
            std::fs::write(directory.join("completion.json"), journal.to_string()).map_err(
                |_| WorkerFailure::new("destination", "Impossibile registrare il completamento."),
            )?;
            match files::move_exclusive(&verified_path, &target) {
                Ok(()) => return Ok(target.to_string_lossy().into()),
                Err(_) if target.exists() => continue,
                Err(_) => {
                    return Err(WorkerFailure::new(
                        "destination",
                        "Impossibile pubblicare il file nella destinazione.",
                    ))
                }
            }
        }
        Err(WorkerFailure::new(
            "destination",
            "Troppi file con lo stesso nome.",
        ))
    }
}

pub async fn wait_cancel(mut receiver: watch::Receiver<bool>) {
    loop {
        if *receiver.borrow() {
            return;
        }
        if receiver.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
    };
    struct Server(Child);
    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[tokio::test]
    async fn real_pipeline_and_crash_after_rename_do_not_duplicate_output() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_owned();
        let temp = tempfile::tempdir().unwrap();
        let fixture = temp.path().join("fixture");
        let destination = temp.path().join("Scaricati è");
        let resources = std::env::var_os("FRAME_TEST_RESOURCES")
            .map(PathBuf::from)
            .unwrap_or(root.join("resources"));
        std::fs::create_dir_all(&fixture).unwrap();
        std::fs::create_dir_all(&destination).unwrap();
        let status = Command::new(resources.join("runtime/bin/ffmpeg.exe"))
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=160x90:rate=10",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440",
                "-t",
                "1",
                "-c:v",
                "mpeg4",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-f",
                "dash",
                "fixture.mpd",
            ])
            .current_dir(&fixture)
            .status()
            .unwrap();
        assert!(status.success());
        let mut server = Server(
            Command::new(resources.join("runtime/python/python.exe"))
                .args(["-I", "-u"])
                .arg(root.join("tests/fixture_server.py"))
                .arg(&fixture)
                .stdout(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let mut port = String::new();
        BufReader::new(server.0.stdout.take().unwrap())
            .read_line(&mut port)
            .unwrap();
        let data = temp.path().join("data");
        let service = Service::new(
            resources.clone(),
            data.clone(),
            destination.clone(),
            |_, _| {},
        )
        .unwrap();
        let url = format!("http://127.0.0.1:{}/fixture.mpd", port.trim());
        let jobs = {
            let mut queue = service.queue.lock().unwrap();
            let options = queue.settings.defaults.clone();
            queue.enqueue(&[json!({"url":url,"title":"Integration: video","filename":"Integration: video"})],options).unwrap();
            queue.claim("baseline").unwrap()
        };
        let (job, cancel) = jobs.into_iter().next().unwrap();
        let output = service.download_job(&job, cancel).await.unwrap();
        assert!(Path::new(&output).is_file());
        assert!(output.contains("Integration_ video"));
        // Simulate termination after atomic rename but before the completed DB commit.
        drop(service);
        let service = Service::new(
            resources.clone(),
            data.clone(),
            destination.clone(),
            |_, _| {},
        )
        .unwrap();
        assert_eq!(service.queue.lock().unwrap().jobs[0].state, "paused");
        service.reconcile().await;
        assert_eq!(service.queue.lock().unwrap().jobs[0].state, "completed");
        let outputs = std::fs::read_dir(&destination)
            .unwrap()
            .flatten()
            .filter(|e| e.file_type().unwrap().is_file())
            .count();
        assert_eq!(outputs, 1);
        assert!(service
            .queue
            .lock()
            .unwrap()
            .claim("baseline")
            .unwrap()
            .is_empty());

        // Exercise a mixed bulk against the installed worker: colliding names and
        // one inaccessible item must not prevent the other files completing.
        let batch = {
            let mut queue = service.queue.lock().unwrap();
            let options = queue.settings.defaults.clone();
            queue.enqueue(&[
                json!({"url":url,"title":"Same name","filename":"Same name"}),
                json!({"url":url,"title":"Same name","filename":"Same name"}),
                json!({"url":format!("http://127.0.0.1:{}/missing.mpd",port.trim()),"title":"Unavailable item"}),
            ], options).unwrap();
            queue.claim("baseline").unwrap()
        };
        let mut tasks = Vec::new();
        for (job, cancel) in batch {
            let service = service.clone();
            tasks.push(tokio::spawn(async move {
                let result = service.download_job(&job, cancel).await;
                service
                    .queue
                    .lock()
                    .unwrap()
                    .finish(&job.id, &job.attempt_id, result)
                    .unwrap();
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        {
            let queue = service.queue.lock().unwrap();
            assert_eq!(
                queue.jobs.iter().filter(|j| j.state == "completed").count(),
                3
            );
            assert_eq!(queue.jobs.iter().filter(|j| j.state == "failed").count(), 1);
            let outputs: std::collections::HashSet<_> = queue
                .jobs
                .iter()
                .filter_map(|j| j.output.as_ref())
                .collect();
            assert!(outputs.len() >= 3);
        }
        let (audio, cancel) = {
            let mut queue = service.queue.lock().unwrap();
            let mut options = queue.settings.defaults.clone();
            options.audio_only = true;
            queue
                .enqueue(&[json!({"url":url,"title":"Audio restart"})], options)
                .unwrap();
            let claimed = queue.claim("baseline").unwrap().remove(0);
            queue.control(&claimed.0.id, "cancel").unwrap();
            claimed
        };
        let cancelled = service.download_job(&audio, cancel).await;
        assert!(cancelled.is_err());
        let retry = {
            let mut queue = service.queue.lock().unwrap();
            queue
                .finish(&audio.id, &audio.attempt_id, cancelled)
                .unwrap();
            assert_eq!(queue.jobs.last().unwrap().state, "cancelled");
            queue.control(&audio.id, "retry").unwrap();
            queue.claim("baseline").unwrap().remove(0)
        };
        let output = service.download_job(&retry.0, retry.1).await.unwrap();
        {
            let mut queue = service.queue.lock().unwrap();
            queue
                .finish(&retry.0.id, &retry.0.attempt_id, Ok(output))
                .unwrap();
            let mut settings = queue.settings.clone();
            settings.concurrent_jobs = 2;
            queue.settings(settings).unwrap();
        }
        drop(service);
        let service = Service::new(resources, data, destination, |_, _| {}).unwrap();
        let queue = service.queue.lock().unwrap();
        assert_eq!(queue.settings.concurrent_jobs, 2);
        assert_eq!(
            queue.jobs.iter().filter(|j| j.state == "completed").count(),
            4
        );
    }
}
