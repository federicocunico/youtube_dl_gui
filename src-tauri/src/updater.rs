use crate::{
    model::now,
    process::{run_worker, RuntimePaths},
    service::Service,
};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::sync::watch;

pub(crate) fn version(service: &Service, environment: &str) -> Result<String, String> {
    let file = if environment == "baseline" {
        service.resources.join("runtime-lock.json")
    } else {
        service
            .data
            .join("engines")
            .join(environment)
            .join("manifest.json")
    };
    let value: Value = serde_json::from_str(
        &std::fs::read_to_string(file).map_err(|_| "Manifest motore mancante.")?,
    )
    .map_err(|_| "Manifest motore non valido.")?;
    value[if environment == "baseline" {
        "engine"
    } else {
        "version"
    }]
    .as_str()
    .map(String::from)
    .ok_or("Versione motore mancante.".into())
}

fn state(service: &Service, mut value: Value) {
    {
        let queue = service.queue.lock().unwrap();
        if let Ok(Some(last)) = queue.store.get::<u64>("last_successful_test") {
            value["last_success"] = json!(last);
        }
        let _ = queue.store.set("last_update_view", &value);
    }
    *service.update.lock().unwrap() = value;
    service.changed();
}

async fn operation(
    service: Arc<Service>,
    mut runtime: RuntimePaths,
    payload: Value,
    seconds: u64,
) -> Result<Value, String> {
    runtime.worker = runtime.worker.with_file_name("update.py");
    let (_tx, cancel) = watch::channel(false);
    let sink = service.clone();
    run_worker(
        runtime,
        "updater".into(),
        uuid::Uuid::new_v4().to_string(),
        "healthcheck",
        payload,
        cancel,
        Duration::from_secs(seconds),
        move |event| {
            if event["event"] == "phase" && event["data"]["state"] == "installing" {
                let mut value = sink.update.lock().unwrap().clone();
                value["phase"] = json!("installing");
                value["candidate"] = event["data"]["version"].clone();
                state(&sink, value);
            }
        },
    )
    .await
    .map_err(|e| e.message)
}

async fn suite(
    service: Arc<Service>,
    runtime: RuntimePaths,
    control: Option<&str>,
) -> Result<Value, String> {
    operation(service.clone(),runtime,json!({"operation":"suite","canaries":service.resources.join("canaries.json"),"test_root":service.data.join("update-tests"),"control_url":control}),125).await
}

pub fn spawn(service: Arc<Service>) {
    tauri::async_runtime::spawn(async move {
        loop {
            if service.queue.lock().unwrap().stopping {
                break;
            }
            let _ = check(service.clone(), false).await;
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}

pub async fn check(service: Arc<Service>, manual: bool) -> Result<Value, String> {
    let Ok(_lock) = service.update_lock.try_lock() else {
        return Ok(json!({"phase":"busy"}));
    };
    let active = service.active();
    let current = version(&service, &active)?;
    let (checked, next, pending, quarantine) = {
        let queue = service.queue.lock().unwrap();
        (
            queue.store.get::<u64>("last_update_check")?.unwrap_or(0),
            queue.store.get::<u64>("next_validation")?.unwrap_or(0),
            queue
                .store
                .get::<Value>("pending_engine")?
                .filter(|v| v.is_object()),
            queue
                .store
                .get::<Vec<String>>("quarantine")?
                .unwrap_or_default(),
        )
    };
    if !check_due(checked, next, pending.is_some(), manual, now()) {
        return Ok(service.update.lock().unwrap().clone());
    }
    state(
        &service,
        json!({"phase":"checking","installed_version":current}),
    );
    let result=async {
        let latest=operation(service.clone(),service.runtime("baseline")?,json!({"operation":"latest","current_version":current}),40).await?;
        service.queue.lock().unwrap().store.set("last_update_check",&now())?;
        let latest_version=latest["version"].as_str().ok_or("Risposta PyPI non valida.")?.to_string();
        if latest["newer"]!=true {return Ok(json!({"phase":"current","installed_version":current,"last_check":now()}));}
        if quarantine.contains(&latest_version) && !manual {return Ok(json!({"phase":"quarantined","installed_version":current,"candidate":latest_version,"message":"Aggiornamento rifiutato in precedenza. Riprova manualmente o attendi una nuova versione."}));}
        let existing=pending.filter(|p|p["version"]==latest_version && p["id"].as_str().is_some_and(|id|uuid::Uuid::parse_str(id).is_ok()));
        let id=existing.as_ref().and_then(|p|p["id"].as_str()).map(String::from).unwrap_or_else(||uuid::Uuid::new_v4().to_string());
        let stage=service.data.join("staging").join(&id);
        if existing.is_none() || !stage.join("manifest.json").is_file() {
            service.queue.lock().unwrap().store.set("validation_retries",&0_u64)?;
            if stage.exists(){safe_remove(&stage,&service.data.join("staging"))?;}
            service.queue.lock().unwrap().store.set("update_journal",&json!({"id":id,"phase":"installing"}))?;
            state(&service,json!({"phase":"installing","installed_version":current,"candidate":latest_version}));
            operation(service.clone(),service.runtime("baseline")?,json!({"operation":"prepare","version":latest_version,"stage":stage,"staging_root":service.data.join("staging"),"baseline_python":service.resources.join("runtime/python")}),600).await?;
        }
        service.queue.lock().unwrap().store.set("pending_engine",&json!({"id":id,"version":latest_version}))?;
        state(&service,json!({"phase":"testing","installed_version":current,"candidate":latest_version}));
        let mut runtime=service.runtime("baseline")?;runtime.python=stage.join("python/python.exe");
        let test=suite(service.clone(),runtime,None).await?;
        if test["status"]!="pass" {
            let control=test["control_url"].as_str();
            let previous=suite(service.clone(),service.runtime(&active)?,control).await.unwrap_or(json!({"status":"inconclusive"}));
            if confirmed_regression(&test,&previous) {
                let mut blocked=quarantine.clone();if !blocked.contains(&latest_version){blocked.push(latest_version.clone());}
                let queue=service.queue.lock().unwrap();queue.store.set("quarantine",&blocked)?;queue.store.set("pending_engine",&Value::Null)?;
                queue.store.log_update(&json!({"state":"rejected","version":latest_version,"test":test,"time":now()}))?;
                drop(queue);safe_remove(&stage,&service.data.join("staging"))?;
                return Ok(json!({"phase":"rejected","installed_version":current,"candidate":latest_version,"message":"Download di test fallito. Conservata la versione precedente.","test":test}));
            }
            let queue=service.queue.lock().unwrap();
            let attempt=queue.store.get::<u64>("validation_retries")?.unwrap_or(0);
            let delay=retry_delay(attempt);
            queue.store.set("validation_retries",&(attempt+1))?;queue.store.set("next_validation",&(now()+delay))?;
            return Ok(json!({"phase":"inconclusive","installed_version":current,"candidate":latest_version,"next_retry":now()+delay,"message":"Test non conclusivo. Resta attiva la versione precedente.","test":test}));
        }
        activate(&service,&id,&active,&latest_version,&stage,&test)?;
        service.notify.notify_one();
        if let Err(message) = cleanup(&service) {
            let _ = service.queue.lock().unwrap().store.log_update(&json!({"state":"cleanup-error","message":message,"time":now()}));
        }
        Ok(json!({"phase":"activated","installed_version":latest_version,"last_success":now(),"test":test}))
    }.await;
    match &result {
        Ok(value) => state(&service, value.clone()),
        Err(message) => {
            let queue = service.queue.lock().unwrap();
            let _ = queue.store.set("last_update_check", &now());
            if queue
                .store
                .get::<Value>("pending_engine")
                .ok()
                .flatten()
                .is_some_and(|v| v.is_object())
            {
                let attempt = queue
                    .store
                    .get::<u64>("validation_retries")
                    .ok()
                    .flatten()
                    .unwrap_or(0);
                let _ = queue.store.set("validation_retries", &(attempt + 1));
                let _ = queue
                    .store
                    .set("next_validation", &(now() + retry_delay(attempt)));
            }
            let _ = queue
                .store
                .log_update(&json!({"state":"error","message":message,"time":now()}));
            drop(queue);
            state(
                &service,
                json!({"phase":"error","installed_version":current,"message":message}),
            );
        }
    }
    result
}

pub async fn recover(service: Arc<Service>, failed_engine: String) -> Result<(), String> {
    let _lock = service.update_lock.lock().await;
    if service.active() != failed_engine || failed_engine == "baseline" {
        return Ok(());
    }
    let previous = service
        .queue
        .lock()
        .unwrap()
        .store
        .get::<String>("previous_engine")?
        .unwrap_or_else(|| "baseline".into());
    let candidate = suite(service.clone(), service.runtime(&failed_engine)?, None).await?;
    if candidate["status"] != "fail" {
        return Ok(());
    }
    let control = suite(
        service.clone(),
        service.runtime(&previous)?,
        candidate["control_url"].as_str(),
    )
    .await?;
    if control["status"] != "pass" {
        return Ok(());
    }
    rollback_confirmed(&service, &failed_engine, &previous, &candidate)?;
    Ok(())
}

fn activate(
    service: &Service,
    id: &str,
    active: &str,
    latest_version: &str,
    stage: &Path,
    test: &Value,
) -> Result<(), String> {
    if test["status"] != "pass"
        || uuid::Uuid::parse_str(id).is_err()
        || stage.parent() != Some(service.data.join("staging").as_path())
    {
        return Err("Candidato non validato o percorso non valido.".into());
    }
    let mut manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(stage.join("manifest.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if manifest["version"] != latest_version || !stage.join("python/python.exe").is_file() {
        return Err("Ambiente candidato incompleto.".into());
    }
    manifest["validated"] = json!(true);
    manifest["test"] = test.clone();
    std::fs::write(stage.join("manifest.json"), manifest.to_string()).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(service.data.join("engines")).map_err(|e| e.to_string())?;
    std::fs::rename(stage, service.data.join("engines").join(id)).map_err(|e| e.to_string())?;
    let report = json!({"state":"activated","version":latest_version,"environment":id,"previous":active,"test":test,"time":now()});
    let mut queue = service.queue.lock().unwrap();
    queue.store.promote(id, active, &report)?;
    queue.store.set("pending_engine", &Value::Null)?;
    queue.store.set("update_journal", &Value::Null)?;
    queue.store.set("validation_retries", &0_u64)?;
    queue.store.set("last_successful_test", &now())?;
    Ok(())
}

fn rollback_confirmed(
    service: &Service,
    failed_engine: &str,
    previous: &str,
    candidate: &Value,
) -> Result<bool, String> {
    let failed_version = version(service, failed_engine)?;
    {
        let mut queue = service.queue.lock().unwrap();
        if queue.store.get::<String>("active_engine")?.as_deref() != Some(failed_engine) {
            return Ok(false);
        }
        let mut blocked = queue
            .store
            .get::<Vec<String>>("quarantine")?
            .unwrap_or_default();
        if !blocked.contains(&failed_version) {
            blocked.push(failed_version.clone());
        }
        let mut jobs = queue.jobs.clone();
        let mut affected = Vec::new();
        for job in &mut jobs {
            if job.engine == failed_engine && job.active() && !job.recovery_retried {
                affected.push(job.id.clone());
                job.state = "queued".into();
                job.engine = previous.into();
                job.recovery_retried = true;
                job.temporary = None;
                job.output = None;
                job.ready_at = 0;
                job.error = None;
            }
        }
        queue.store.commit_engine_change(previous,"baseline",&json!({"state":"rollback","from":failed_version,"restored":version(service,previous)?,"test":candidate,"time":now()}),Some(&jobs),Some(&blocked))?;
        queue.jobs = jobs;
        for id in affected {
            if let Some(running) = queue.running.get(&id) {
                let _ = running.cancel.send(true);
            }
        }
    }
    state(
        service,
        json!({"phase":"rollback","installed_version":version(service,previous)?,"rejected_version":failed_version,"message":"Ripristinata automaticamente la versione precedente dopo una regressione confermata."}),
    );
    service.notify.notify_one();
    Ok(true)
}

fn retry_delay(attempt: u64) -> u64 {
    match attempt {
        0 => 3600,
        1 => 21600,
        _ => 86400,
    }
}
fn check_due(checked: u64, next: u64, pending: bool, manual: bool, time: u64) -> bool {
    manual
        || if pending {
            time >= next
        } else {
            time >= checked.saturating_add(86400)
        }
}
fn confirmed_regression(candidate: &Value, control: &Value) -> bool {
    candidate["status"] == "fail" && control["status"] == "pass"
}

fn safe_remove(path: &Path, parent: &Path) -> Result<(), String> {
    if path.parent() != Some(parent)
        || path
            .file_name()
            .and_then(|p| p.to_str())
            .is_none_or(|p| uuid::Uuid::parse_str(p).is_err())
    {
        return Err("Percorso di pulizia non valido.".into());
    }
    if path.exists() {
        let resolved = path.canonicalize().map_err(|e| e.to_string())?;
        let expected = parent.canonicalize().map_err(|e| e.to_string())?;
        if resolved.parent() != Some(expected.as_path())
            || std::fs::symlink_metadata(path)
                .map_err(|e| e.to_string())?
                .file_type()
                .is_symlink()
        {
            return Err("Directory di pulizia esterna al percorso previsto.".into());
        }
        std::fs::remove_dir_all(&resolved).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn validated_environment(data: &Path, id: &str) -> bool {
    if id == "baseline" {
        return true;
    }
    if uuid::Uuid::parse_str(id).is_err() {
        return false;
    }
    let path = data.join("engines").join(id);
    path.join("python/python.exe").is_file()
        && std::fs::read_to_string(path.join("manifest.json"))
            .ok()
            .and_then(|v| serde_json::from_str::<Value>(&v).ok())
            .is_some_and(|v| v["validated"] == true)
}

pub fn recover_startup(queue: &mut crate::queue::Queue, data: &Path) -> Result<Value, String> {
    let active = queue
        .store
        .get::<String>("active_engine")?
        .unwrap_or_else(|| "baseline".into());
    if !validated_environment(data, &active) {
        let previous = queue
            .store
            .get::<String>("previous_engine")?
            .filter(|id| validated_environment(data, id))
            .unwrap_or_else(|| "baseline".into());
        queue.store.promote(
            &previous,
            "baseline",
            &json!({"state":"rollback","reason":"Ambiente incompleto al riavvio","time":now()}),
        )?;
        for job in &mut queue.jobs {
            if !job.terminal() && job.engine == active {
                job.engine = previous.clone();
                job.temporary = None;
                job.output = None;
                job.state = "paused".into();
            }
        }
        queue.store.save_jobs(&queue.jobs)?;
    }
    let pending = queue
        .store
        .get::<Value>("pending_engine")?
        .filter(|v| v.is_object());
    if let Some(pending) = &pending {
        let valid = pending["id"]
            .as_str()
            .filter(|id| uuid::Uuid::parse_str(id).is_ok())
            .is_some_and(|id| {
                data.join("staging")
                    .join(id)
                    .join("manifest.json")
                    .is_file()
            });
        if !valid {
            queue.store.set("pending_engine", &Value::Null)?;
        }
    }
    if let Some(journal) = queue.store.get::<Value>("update_journal")? {
        if let Some(id) = journal["id"]
            .as_str()
            .filter(|id| uuid::Uuid::parse_str(id).is_ok())
        {
            let retained = pending.as_ref().is_some_and(|v| v["id"] == id);
            if !retained {
                safe_remove(&data.join("staging").join(id), &data.join("staging"))?;
            }
        }
        queue.store.set("update_journal", &Value::Null)?;
    }
    let mut view = queue
        .store
        .get::<Value>("last_update_view")?
        .unwrap_or(json!({"phase":"idle"}));
    if matches!(
        view["phase"].as_str(),
        Some("checking" | "installing" | "testing")
    ) {
        view["phase"] = json!("idle");
        view["message"] =
            json!("Controllo precedente interrotto. La versione validata è ancora attiva.");
    }
    Ok(view)
}

fn cleanup(service: &Service) -> Result<(), String> {
    let queue = service.queue.lock().unwrap();
    let mut protected: std::collections::HashSet<String> = queue
        .jobs
        .iter()
        .filter(|j| !j.terminal())
        .map(|j| j.engine.clone())
        .collect();
    for key in ["active_engine", "previous_engine"] {
        if let Some(value) = queue.store.get::<String>(key)? {
            protected.insert(value);
        }
    }
    // Running jobs retain references even after a user pause/cancel until their worker exits.
    for running in queue.running.values() {
        protected.insert(running.engine.clone());
    }
    drop(queue);
    let directory = service.data.join("engines");
    if let Ok(entries) = std::fs::read_dir(&directory) {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|t| t.is_dir())
                && !protected.contains(&entry.file_name().to_string_lossy().to_string())
            {
                safe_remove(&entry.path(), &directory)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, Arc<Service>) {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_owned();
        let resources = std::env::var_os("FRAME_TEST_RESOURCES")
            .map(std::path::PathBuf::from)
            .unwrap_or(root.join("resources"));
        let temp = tempfile::tempdir().unwrap();
        let service = Service::new(
            resources,
            temp.path().join("data"),
            temp.path().to_owned(),
            |_, _| {},
        )
        .unwrap();
        (temp, service)
    }

    fn candidate(service: &Service) -> (String, std::path::PathBuf) {
        let id = uuid::Uuid::new_v4().to_string();
        let stage = service.data.join("staging").join(&id);
        std::fs::create_dir_all(stage.join("python")).unwrap();
        std::fs::write(stage.join("python/python.exe"), b"unit fixture").unwrap();
        std::fs::write(
            stage.join("manifest.json"),
            json!({"version":"2027.1.1","validated":false}).to_string(),
        )
        .unwrap();
        (id, stage)
    }

    #[test]
    fn stable_check_schedule_and_differential_verdicts() {
        assert!(!check_due(100, 0, false, false, 101));
        assert!(check_due(100, 0, false, true, 101));
        assert!(check_due(100, 0, false, false, 86500));
        assert!(!check_due(100, 1000, true, false, 999));
        assert!(check_due(100, 1000, true, false, 1000));
        assert!(!check_due(100, 100000, true, false, 90000));
        assert_eq!(
            [
                retry_delay(0),
                retry_delay(1),
                retry_delay(2),
                retry_delay(99)
            ],
            [3600, 21600, 86400, 86400]
        );
        assert!(confirmed_regression(
            &json!({"status":"fail"}),
            &json!({"status":"pass"})
        ));
        assert!(!confirmed_regression(
            &json!({"status":"fail"}),
            &json!({"status":"fail"})
        ));
        assert!(!confirmed_regression(
            &json!({"status":"inconclusive"}),
            &json!({"status":"pass"})
        ));
    }

    #[test]
    fn activation_requires_pass_and_preserves_existing_job_environments() {
        let (_temp, service) = fixture();
        let (id, stage) = candidate(&service);
        assert!(activate(
            &service,
            &id,
            "baseline",
            "2027.1.1",
            &stage,
            &json!({"status":"inconclusive"})
        )
        .is_err());
        assert_eq!(service.active(), "baseline");
        let running = {
            let mut queue = service.queue.lock().unwrap();
            let options = queue.settings.defaults.clone();
            queue
                .enqueue(
                    &vec![json!({"url":"https://youtu.be/el3E4MbxRqQ","title":"fixture"}); 4],
                    options,
                )
                .unwrap();
            queue.claim("baseline").unwrap()
        };
        activate(
            &service,
            &id,
            "baseline",
            "2027.1.1",
            &stage,
            &json!({"status":"pass"}),
        )
        .unwrap();
        assert_eq!(service.active(), id);
        assert!(!stage.exists());
        assert!(validated_environment(&service.data, &id));
        let mut queue = service.queue.lock().unwrap();
        assert!(queue.running.values().all(|r| r.engine == "baseline"));
        queue
            .finish(
                &running[0].0.id,
                &running[0].0.attempt_id,
                Ok("complete.mp4".into()),
            )
            .unwrap();
        let next = queue.claim(&id).unwrap();
        assert_eq!(next[0].0.engine, id);
    }

    #[test]
    fn rollback_is_atomic_single_retry_and_retains_running_environment() {
        let (_temp, service) = fixture();
        let (id, stage) = candidate(&service);
        activate(
            &service,
            &id,
            "baseline",
            "2027.1.1",
            &stage,
            &json!({"status":"pass"}),
        )
        .unwrap();
        let running = {
            let mut queue = service.queue.lock().unwrap();
            let options = queue.settings.defaults.clone();
            queue
                .enqueue(
                    &vec![json!({"url":"https://youtu.be/el3E4MbxRqQ","title":"fixture"}); 3],
                    options,
                )
                .unwrap();
            queue.claim(&id).unwrap()
        };
        service
            .queue
            .lock()
            .unwrap()
            .finish(
                &running[0].0.id,
                &running[0].0.attempt_id,
                Ok("preserved.mp4".into()),
            )
            .unwrap();
        assert!(rollback_confirmed(&service, &id, "baseline", &json!({"status":"fail"})).unwrap());
        assert_eq!(service.active(), "baseline");
        assert!(*running[1].1.borrow());
        {
            let queue = service.queue.lock().unwrap();
            assert_eq!(queue.jobs[0].state, "completed");
            assert_eq!(queue.jobs[0].output.as_deref(), Some("preserved.mp4"));
            assert!(queue.jobs[1].recovery_retried);
            assert_eq!(queue.jobs[1].engine, "baseline");
            assert_eq!(queue.store.jobs().unwrap()[1].engine, "baseline");
            assert!(queue
                .store
                .get::<Vec<String>>("quarantine")
                .unwrap()
                .unwrap()
                .contains(&"2027.1.1".into()));
        }
        cleanup(&service).unwrap();
        assert!(service.data.join("engines").join(&id).exists());
        assert!(!rollback_confirmed(&service, &id, "baseline", &json!({"status":"fail"})).unwrap());
    }

    #[test]
    fn incomplete_startup_environment_restores_baseline() {
        let (temp, service) = fixture();
        let (id, stage) = candidate(&service);
        std::fs::create_dir_all(service.data.join("engines")).unwrap();
        std::fs::rename(&stage, service.data.join("engines").join(&id)).unwrap();
        service
            .queue
            .lock()
            .unwrap()
            .store
            .set("active_engine", &id)
            .unwrap();
        let resources = service.resources.clone();
        let data = service.data.clone();
        drop(service);
        let service = Service::new(resources, data, temp.path().to_owned(), |_, _| {}).unwrap();
        assert_eq!(service.active(), "baseline");
        assert!(service
            .queue
            .lock()
            .unwrap()
            .store
            .history()
            .unwrap()
            .iter()
            .any(|h| h["state"] == "rollback"));
    }

    #[tokio::test]
    async fn updater_lock_prevents_parallel_installation() {
        let (_temp, service) = fixture();
        let _guard = service.update_lock.lock().await;
        let result = check(service.clone(), true).await.unwrap();
        assert_eq!(result["phase"], "busy");
    }

    #[test]
    fn interrupted_staging_and_rename_do_not_promote_or_reset_backoff() {
        for phase in ["installing", "testing", "renamed"] {
            let (temp, service) = fixture();
            let (id, stage) = candidate(&service);
            if phase == "renamed" {
                std::fs::create_dir_all(service.data.join("engines")).unwrap();
                std::fs::rename(&stage, service.data.join("engines").join(&id)).unwrap();
            }
            {
                let queue = service.queue.lock().unwrap();
                queue
                    .store
                    .set("update_journal", &json!({"id":id,"phase":phase}))
                    .unwrap();
                queue
                    .store
                    .set("last_update_view", &json!({"phase":"testing"}))
                    .unwrap();
                queue.store.set("last_update_check", &100_u64).unwrap();
                queue.store.set("next_validation", &100000_u64).unwrap();
                if phase == "testing" {
                    queue
                        .store
                        .set("pending_engine", &json!({"id":id,"version":"2027.1.1"}))
                        .unwrap();
                }
            }
            let resources = service.resources.clone();
            let data = service.data.clone();
            drop(service);
            let service = Service::new(resources, data, temp.path().to_owned(), |_, _| {}).unwrap();
            assert_eq!(service.active(), "baseline");
            assert_eq!(stage.exists(), phase == "testing");
            let queue = service.queue.lock().unwrap();
            assert_eq!(
                queue.store.get::<u64>("next_validation").unwrap(),
                Some(100000)
            );
            assert_eq!(service.update.lock().unwrap()["phase"], "idle");
            assert!(queue
                .store
                .get::<Value>("update_journal")
                .unwrap()
                .unwrap()
                .is_null());
        }
    }

    #[tokio::test]
    #[ignore = "Real pip update, YouTube downloads and induced regression; release gate"]
    async fn live_update_and_rollback() {
        let (_temp, service) = fixture();
        let version = version(&service, "baseline").unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let stage = service.data.join("staging").join(&id);
        operation(service.clone(),service.runtime("baseline").unwrap(),json!({"operation":"prepare","version":version,"stage":stage,"staging_root":service.data.join("staging"),"baseline_python":service.resources.join("runtime/python")}),600).await.unwrap();
        let mut candidate = service.runtime("baseline").unwrap();
        candidate.python = stage.join("python/python.exe");
        let validated = suite(service.clone(), candidate, None).await.unwrap();
        assert_eq!(validated["status"], "pass");
        let url = "https://www.youtube.com/watch?v=el3E4MbxRqQ";
        let existing = {
            let mut queue = service.queue.lock().unwrap();
            let mut options = queue.settings.defaults.clone();
            options.video_quality = 360;
            queue
                .enqueue(
                    &[json!({"url":url,"title":"Existing baseline job"})],
                    options,
                )
                .unwrap();
            queue.claim("baseline").unwrap().remove(0)
        };
        let background = service.clone();
        let old_job = existing.0.clone();
        let old_cancel = existing.1.clone();
        let old_task =
            tokio::spawn(async move { background.download_job(&old_job, old_cancel).await });
        tokio::task::yield_now().await;
        assert!(!old_task.is_finished());
        activate(&service, &id, "baseline", &version, &stage, &validated).unwrap();
        assert_eq!(
            service.queue.lock().unwrap().running[&existing.0.id].engine,
            "baseline"
        );
        let module = service
            .data
            .join("engines")
            .join(&id)
            .join("python/Lib/site-packages/yt_dlp/YoutubeDL.py");
        let mut source = std::fs::read_to_string(&module).unwrap();
        source.push_str("\ndef _frame_induced_regression(*args, **kwargs):\n    raise RuntimeError('Induced integration regression')\nYoutubeDL.extract_info = _frame_induced_regression\n");
        std::fs::write(&module, source).unwrap();
        let affected = {
            let mut queue = service.queue.lock().unwrap();
            let mut options = queue.settings.defaults.clone();
            options.video_quality = 360;
            queue
                .enqueue(&[json!({"url":url,"title":"Candidate job"})], options)
                .unwrap();
            queue.claim(&id).unwrap().remove(0)
        };
        let failed = service.download_job(&affected.0, affected.1.clone()).await;
        assert!(failed.is_err());
        recover(service.clone(), id.clone()).await.unwrap();
        assert_eq!(service.active(), "baseline");
        service
            .queue
            .lock()
            .unwrap()
            .finish(&affected.0.id, &affected.0.attempt_id, failed)
            .unwrap();
        let old_output = old_task.await.unwrap().unwrap();
        service
            .queue
            .lock()
            .unwrap()
            .finish(
                &existing.0.id,
                &existing.0.attempt_id,
                Ok(old_output.clone()),
            )
            .unwrap();
        let retried = service
            .queue
            .lock()
            .unwrap()
            .claim("baseline")
            .unwrap()
            .remove(0);
        assert!(retried.0.recovery_retried);
        let output = service.download_job(&retried.0, retried.1).await.unwrap();
        service
            .queue
            .lock()
            .unwrap()
            .finish(&retried.0.id, &retried.0.attempt_id, Ok(output))
            .unwrap();
        assert!(std::path::Path::new(&old_output).is_file());
        assert!(service
            .queue
            .lock()
            .unwrap()
            .jobs
            .iter()
            .all(|job| job.state == "completed"));
        assert!(!rollback_confirmed(&service, &id, "baseline", &json!({"status":"fail"})).unwrap());
        if let Some(path) = std::env::var_os("FRAME_UPDATE_REPORT") {
            std::fs::write(path,json!({"status":"pass","version":version,"baseline_active_during_promotion":true,"automatic_rollback":true,"recovery_retries":1}).to_string()).unwrap();
        }
    }
    #[tokio::test]
    #[ignore = "Requires live YouTube; mandatory release gate"]
    async fn live_canaries() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_owned();
        let resources = std::env::var_os("FRAME_TEST_RESOURCES")
            .map(std::path::PathBuf::from)
            .unwrap_or(root.join("resources"));
        let temp = tempfile::tempdir().unwrap();
        let service = Service::new(
            resources.clone(),
            temp.path().join("data"),
            temp.path().to_owned(),
            |_, _| {},
        )
        .unwrap();
        let manifest: Value = serde_json::from_str(
            &std::fs::read_to_string(resources.join("canaries.json")).unwrap(),
        )
        .unwrap();
        let mut reports = Vec::new();
        for video in manifest["videos"].as_array().unwrap() {
            let report = suite(
                service.clone(),
                service.runtime("baseline").unwrap(),
                video["url"].as_str(),
            )
            .await
            .unwrap();
            println!("Canary {}: {}", video["url"], report);
            reports.push(report);
        }
        if let Some(path) = std::env::var_os("FRAME_LIVE_REPORT") {
            std::fs::write(path, serde_json::to_string_pretty(&reports).unwrap()).unwrap();
        }
        assert!(
            reports.iter().all(|r| r["status"] == "pass"),
            "Live validation failed or was inconclusive"
        );
    }
}
