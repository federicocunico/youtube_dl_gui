use crate::process::WorkerFailure;
use serde::{Deserialize, Serialize};

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct DownloadOptions {
    pub destination: String,
    pub audio_only: bool,
    pub video_quality: u32,
    pub container: String,
    pub audio_format: String,
    pub audio_bitrate: u32,
    pub fragments: usize,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            destination: String::new(),
            audio_only: false,
            video_quality: 0,
            container: "auto".into(),
            audio_format: "original".into(),
            audio_bitrate: 192,
            fragments: 4,
        }
    }
}

impl DownloadOptions {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=8).contains(&self.fragments) {
            return Err("Parallelismo frammenti: scegli un valore tra 1 e 8.".into());
        }
        if ![0, 144, 240, 360, 480, 720, 1080, 1440, 2160, 4320].contains(&self.video_quality) {
            return Err("Qualità video non supportata.".into());
        }
        if !["auto", "mp4", "mkv", "webm"].contains(&self.container.as_str()) {
            return Err("Contenitore non supportato.".into());
        }
        if !["original", "mp3", "aac"].contains(&self.audio_format.as_str()) {
            return Err("Formato audio non supportato.".into());
        }
        if ![128, 192, 256, 320].contains(&self.audio_bitrate) {
            return Err("Bitrate audio non supportato.".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub defaults: DownloadOptions,
    pub concurrent_jobs: usize,
    pub theme: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            defaults: DownloadOptions::default(),
            concurrent_jobs: 3,
            theme: "dark".into(),
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        self.defaults.validate()?;
        if !(1..=8).contains(&self.concurrent_jobs) {
            return Err("Download simultanei: scegli un valore tra 1 e 8.".into());
        }
        if !["dark", "light", "system"].contains(&self.theme.as_str()) {
            return Err("Tema non valido.".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Progress {
    pub downloaded: u64,
    pub total: Option<u64>,
    pub speed: Option<f64>,
    pub eta: Option<f64>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub url: String,
    pub title: String,
    pub channel: String,
    pub thumbnail: Option<String>,
    pub filename: String,
    pub options: DownloadOptions,
    pub state: String,
    pub position: i64,
    pub progress: Progress,
    #[serde(default)]
    pub selection: Option<serde_json::Value>,
    pub output: Option<String>,
    pub temporary: Option<String>,
    pub engine: String,
    pub attempt_id: String,
    pub retries: u32,
    pub recovery_retried: bool,
    pub error: Option<WorkerFailure>,
    pub updated_at: u64,
    pub ready_at: u64,
}
impl Job {
    pub fn active(&self) -> bool {
        ["resolving", "downloading", "muxing"].contains(&self.state.as_str())
    }
    pub fn terminal(&self) -> bool {
        ["completed", "failed", "cancelled"].contains(&self.state.as_str())
    }
}
