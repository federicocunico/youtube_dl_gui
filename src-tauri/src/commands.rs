use crate::{
    files,
    model::{DownloadOptions, Settings},
    process::run_worker,
    service::{wait_cancel, Service},
    urls,
};
use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{Manager, State, WebviewUrl, WebviewWindowBuilder, Window};
use tokio::sync::watch;

fn local(window: &Window) -> Result<(), String> {
    local_label(window.label())
}

fn local_label(label: &str) -> Result<(), String> {
    if label == "main" {
        Ok(())
    } else {
        Err("Comando disponibile solo nella finestra locale.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remote_windows_cannot_use_local_commands_or_file_permissions() {
        assert!(local_label("main").is_ok());
        for label in ["preview-123", "remote", "", "main-remote"] {
            assert!(local_label(label).is_err());
        }
        let capability: Value =
            serde_json::from_str(include_str!("../capabilities/main.json")).unwrap();
        assert_eq!(capability["windows"], json!(["main"]));
        assert!(capability.get("remote").is_none());
        assert!(!capability["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p.as_str().is_some_and(|s| s.starts_with("fs:"))));
    }
}

#[tauri::command]
pub fn snapshot(window: Window, service: State<'_, Arc<Service>>) -> Result<Value, String> {
    local(&window)?;
    service.snapshot()
}

#[tauri::command]
pub fn save_settings(
    window: Window,
    service: State<'_, Arc<Service>>,
    settings: Settings,
) -> Result<(), String> {
    local(&window)?;
    files::destination(&settings.defaults.destination)?;
    service.queue.lock().unwrap().settings(settings)?;
    service.changed();
    service.notify.notify_one();
    Ok(())
}

#[tauri::command]
pub fn control_job(
    window: Window,
    service: State<'_, Arc<Service>>,
    id: String,
    action: String,
) -> Result<(), String> {
    local(&window)?;
    let removed = {
        let mut queue = service.queue.lock().unwrap();
        let removed = if action == "remove" {
            queue.jobs.iter().find(|j| j.id == id).cloned()
        } else {
            None
        };
        queue.control(&id, &action)?;
        removed
    };
    if let Some(job) = removed {
        if let Some(temp) = job.temporary {
            let _ = files::clean_temporary(
                std::path::Path::new(&temp),
                std::path::Path::new(&job.options.destination),
                &job.id,
            );
        }
    }
    service.changed();
    service.notify.notify_one();
    Ok(())
}

#[tauri::command]
pub fn preview_output(
    window: Window,
    destination: String,
    filename: String,
    extension: String,
) -> Result<Value, String> {
    local(&window)?;
    let directory = files::destination(&destination)?;
    for suffix in 0..10000 {
        let target = files::candidate(&directory, &filename, &extension, suffix)?;
        if !target.exists() {
            return Ok(
                json!({"filename":target.file_name().unwrap_or_default().to_string_lossy(),"path":target}),
            );
        }
    }
    Err("Troppi file con lo stesso nome.".into())
}

#[tauri::command]
pub fn reorder_jobs(
    window: Window,
    service: State<'_, Arc<Service>>,
    ids: Vec<String>,
) -> Result<(), String> {
    local(&window)?;
    service.queue.lock().unwrap().reorder(&ids)?;
    service.changed();
    Ok(())
}

#[derive(Deserialize)]
pub struct EnqueueItem {
    media: Value,
    options: DownloadOptions,
}

#[tauri::command]
pub async fn enqueue(
    window: Window,
    service: State<'_, Arc<Service>>,
    items: Vec<EnqueueItem>,
) -> Result<Vec<String>, String> {
    local(&window)?;
    for item in &items {
        item.options.validate()?;
        files::destination(&item.options.destination)?;
        let url = urls::normalize(item.media["url"].as_str().ok_or("URL mancante.")?)?;
        if url.playlist {
            return Err("Seleziona i singoli elementi della playlist.".into());
        }
    }
    let mut validated = Vec::new();
    for mut item in items {
        let slots = service.queue.lock().unwrap().metadata_slots.clone();
        let _permit = slots
            .acquire_owned()
            .await
            .map_err(|_| "Analisi interrotta.")?;
        let (_tx, cancel) = watch::channel(false);
        let detail = run_worker(
            service.runtime(&service.active())?,
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
            "inspect",
            json!({"url":item.media["url"],"options":item.options}),
            cancel,
            Duration::from_secs(90),
            |_| {},
        )
        .await
        .map_err(|e| e.message)?;
        let name = files::filename(item.media["filename"].as_str().unwrap_or("Video"));
        item.media = detail;
        item.media["filename"] = json!(name);
        validated.push(item);
    }
    let mut ids = Vec::new();
    {
        let mut queue = service.queue.lock().unwrap();
        for item in validated {
            ids.extend(queue.enqueue(&[item.media], item.options)?);
        }
    }
    service.changed();
    service.notify.notify_one();
    Ok(ids)
}

#[tauri::command]
pub fn discover(
    window: Window,
    service: State<'_, Arc<Service>>,
    id: String,
    input: String,
    options: DownloadOptions,
) -> Result<(), String> {
    local(&window)?;
    options.validate()?;
    uuid::Uuid::parse_str(&id).map_err(|_| "Identificatore analisi non valido.")?;
    if input.len() > 1024 * 1024 {
        return Err("Elenco URL troppo grande.".into());
    }
    let (tx, rx) = watch::channel(false);
    service.discoveries.lock().unwrap().insert(id.clone(), tx);
    let service = service.inner().clone();
    tauri::async_runtime::spawn(async move {
        let seen = Arc::new(Mutex::new(HashSet::<String>::new()));
        let mut work = tokio::task::JoinSet::new();
        for (raw, normalized, _) in urls::import_urls(&input) {
            let link = match normalized {
                Ok(v) => v,
                Err(message) => {
                    (service.emit)(
                        "discovery",
                        json!({"session":id,"item":{"id":uuid::Uuid::new_v4().to_string(),"source":raw,"error":message,"available":false}}),
                    );
                    continue;
                }
            };
            let service = service.clone();
            let session = id.clone();
            let cancel = rx.clone();
            let options = options.clone();
            let seen = seen.clone();
            work.spawn(async move {
                let metadata=service.queue.lock().unwrap().metadata_slots.clone();
                let permit=tokio::select!{p=metadata.acquire_owned()=>p.ok(),_=wait_cancel(cancel.clone())=>None};if permit.is_none(){return;}
                let runtime=match service.runtime(&service.active()){Ok(r)=>r,Err(message)=>{(service.emit)("discovery",json!({"session":session,"item":{"id":link.identity,"error":message,"available":false}}));return;}};
                let sink=service.clone();let session_copy=session.clone();let seen_copy=seen.clone();
                let result=run_worker(runtime,session.clone(),uuid::Uuid::new_v4().to_string(),if link.playlist{"enumerate"}else{"inspect"},json!({"url":link.url,"options":options}),cancel,Duration::from_secs(if link.playlist{3600}else{90}),move |event|{
                    if event["event"]=="item" {let mut item=event["data"].clone();let identity=item["id"].as_str().unwrap_or("").to_string();item["duplicate"]=json!(!identity.is_empty()&&!seen_copy.lock().unwrap().insert(identity));(sink.emit)("discovery",json!({"session":session_copy,"item":item}));}
                }).await;
                match result {
                    Ok(mut item) if !link.playlist=>{item["available"]=json!(true);item["duplicate"]=json!(!seen.lock().unwrap().insert(link.identity));(service.emit)("discovery",json!({"session":session,"item":item}));},
                    Err(error) if error.code!="cancelled"=>(service.emit)("discovery",json!({"session":session,"item":{"id":uuid::Uuid::new_v4().to_string(),"source":link.url,"error":error.message,"available":false}})),
                    _=>{},
                }
            });
        }
        while work.join_next().await.is_some() {}
        service.discoveries.lock().unwrap().remove(&id);
        (service.emit)("discovery", json!({"session":id,"done":true}));
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_discovery(
    window: Window,
    service: State<'_, Arc<Service>>,
    id: String,
) -> Result<(), String> {
    local(&window)?;
    if let Some(cancel) = service.discoveries.lock().unwrap().get(&id) {
        let _ = cancel.send(true);
    }
    Ok(())
}

#[tauri::command]
pub async fn inspect_media(
    window: Window,
    service: State<'_, Arc<Service>>,
    url: String,
    options: DownloadOptions,
) -> Result<Value, String> {
    local(&window)?;
    options.validate()?;
    let link = urls::normalize(&url)?;
    let metadata = service.queue.lock().unwrap().metadata_slots.clone();
    let _permit = metadata
        .acquire_owned()
        .await
        .map_err(|_| "Analisi interrotta.")?;
    let (_tx, cancel) = watch::channel(false);
    run_worker(
        service.runtime(&service.active())?,
        uuid::Uuid::new_v4().to_string(),
        uuid::Uuid::new_v4().to_string(),
        "inspect",
        json!({"url":link.url,"options":options}),
        cancel,
        Duration::from_secs(90),
        |_| {},
    )
    .await
    .map_err(|e| e.message)
}

#[tauri::command]
pub async fn thumbnail(window: Window, url: String) -> Result<String, String> {
    local(&window)?;
    let url = url::Url::parse(&url).map_err(|_| "Immagine non valida.")?;
    if url.scheme() != "https"
        || !matches!(
            url.host_str(),
            Some("i.ytimg.com" | "img.youtube.com" | "yt3.ggpht.com" | "yt3.googleusercontent.com")
        )
        || url.port().is_some()
        || !url.username().is_empty()
    {
        return Err("Origine immagine non consentita.".into());
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| "Impossibile caricare l'immagine.")?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| "Thumbnail non disponibile.")?;
    if !response.status().is_success() {
        return Err("Thumbnail non disponibile.".into());
    }
    let mime = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .to_string();
    if !["image/jpeg", "image/png", "image/webp"].contains(&mime.as_str()) {
        return Err("Formato immagine non supportato.".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Immagine non disponibile.")?
    {
        if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
            return Err("Immagine troppo grande.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

fn shell_open(value: &str) -> Result<(), String> {
    #[cfg(windows)]
    unsafe {
        let open: Vec<u16> = "open\0".encode_utf16().collect();
        let value: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
        let result = windows_sys::Win32::UI::Shell::ShellExecuteW(
            std::ptr::null_mut(),
            open.as_ptr(),
            value.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        );
        if (result as isize) <= 32 {
            return Err("Impossibile aprire il file o il browser.".into());
        }
    }
    Ok(())
}

#[tauri::command]
pub fn open_youtube(window: Window, url: String) -> Result<(), String> {
    local(&window)?;
    shell_open(&urls::normalize(&url)?.url)
}

#[tauri::command]
pub fn open_output(
    window: Window,
    service: State<'_, Arc<Service>>,
    id: String,
    folder: bool,
) -> Result<(), String> {
    local(&window)?;
    let path = {
        let queue = service.queue.lock().unwrap();
        let job = queue
            .jobs
            .iter()
            .find(|j| j.id == id && j.state == "completed")
            .ok_or("File completato non trovato.")?;
        job.output.clone().ok_or("Percorso non disponibile.")?
    };
    let path = std::path::Path::new(&path);
    if !path.is_file() {
        return Err("Il file è stato spostato o eliminato.".into());
    }
    if folder {
        shell_open(
            &path
                .parent()
                .ok_or("Cartella non disponibile.")?
                .to_string_lossy(),
        )
    } else {
        shell_open(&path.to_string_lossy())
    }
}

#[tauri::command]
pub async fn preview(window: Window, id: String) -> Result<(), String> {
    local(&window)?;
    let link = urls::normalize(&format!("https://youtu.be/{id}"))?;
    WebviewWindowBuilder::new(
        window.app_handle(),
        format!("preview-{}", uuid::Uuid::new_v4()),
        WebviewUrl::External(
            format!(
                "https://www.youtube-nocookie.com/embed/{}?autoplay=0",
                link.identity
            )
            .parse()
            .unwrap(),
        ),
    )
    .title("Anteprima YouTube — YT Saver")
    .inner_size(900., 560.)
    .on_navigation(|url| {
        url.scheme() == "https"
            && matches!(
                url.host_str(),
                Some("www.youtube-nocookie.com" | "www.youtube.com")
            )
    })
    .build()
    .map_err(|_| "Anteprima non disponibile. Usa Apri su YouTube.")?;
    Ok(())
}

#[tauri::command]
pub async fn check_update(
    window: Window,
    service: State<'_, Arc<Service>>,
) -> Result<Value, String> {
    local(&window)?;
    crate::updater::check(service.inner().clone(), true).await
}

#[tauri::command]
pub async fn shutdown(window: Window, service: State<'_, Arc<Service>>) -> Result<(), String> {
    local(&window)?;
    service.queue.lock().unwrap().stop()?;
    for cancel in service.discoveries.lock().unwrap().values() {
        let _ = cancel.send(true);
    }
    service.notify.notify_one();
    for _ in 0..50 {
        if service.queue.lock().unwrap().running.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    window.app_handle().exit(0);
    Ok(())
}
