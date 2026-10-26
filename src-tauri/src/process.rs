use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::watch,
};

const MAX_EVENT: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct RuntimePaths {
    pub python: PathBuf,
    pub worker: PathBuf,
    pub bin: PathBuf,
}

impl RuntimePaths {
    pub fn bundled(resources: &Path) -> Result<Self, String> {
        let root = resources
            .canonicalize()
            .map_err(|_| "Risorse applicative mancanti.")?;
        let mut paths = Self {
            python: root.join("runtime/python/python.exe"),
            worker: root.join("worker/main.py"),
            bin: root.join("runtime/bin"),
        };
        #[cfg(debug_assertions)]
        if !paths.worker.is_file() {
            paths.worker = root.parent().unwrap_or(&root).join("worker/main.py");
        }
        paths.verify()?;
        Ok(paths)
    }

    pub fn verify(&self) -> Result<(), String> {
        for path in [
            &self.python,
            &self.worker,
            &self.bin.join("ffmpeg.exe"),
            &self.bin.join("ffprobe.exe"),
            &self.bin.join("deno.exe"),
        ] {
            if !path.is_absolute() || !path.is_file() {
                return Err(format!(
                    "Componente incluso mancante: {}. Ripristina l'installazione.",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerFailure {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub retryable: bool,
    #[serde(default)]
    pub retry_after: Option<u64>,
}

impl WorkerFailure {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable: false,
            retry_after: None,
        }
    }
}

impl From<std::io::Error> for WorkerFailure {
    fn from(_: std::io::Error) -> Self {
        Self::new("runtime", "Errore di comunicazione con il motore locale.")
    }
}

#[cfg(windows)]
pub struct ProcessTree(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
unsafe impl Send for ProcessTree {}

#[cfg(windows)]
impl ProcessTree {
    pub fn assign(pid: u32) -> Result<Self, WorkerFailure> {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::{JobObjects::*, Threading::*},
        };
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(WorkerFailure::new(
                    "runtime",
                    "Impossibile creare il gruppo processi.",
                ));
            }
            let tree = Self(job);
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of_val(&info) as u32,
            ) == 0
            {
                return Err(WorkerFailure::new(
                    "runtime",
                    "Impossibile proteggere il gruppo processi.",
                ));
            }
            let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
            if process.is_null() {
                return Err(WorkerFailure::new(
                    "runtime",
                    "Processo locale non disponibile.",
                ));
            }
            let assigned = AssignProcessToJobObject(job, process);
            CloseHandle(process);
            if assigned == 0 {
                return Err(WorkerFailure::new(
                    "runtime",
                    "Impossibile supervisionare il gruppo processi.",
                ));
            }
            Ok(tree)
        }
    }
}

#[cfg(windows)]
impl Drop for ProcessTree {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(not(windows))]
pub struct ProcessTree;
#[cfg(not(windows))]
impl ProcessTree {
    pub fn assign(_: u32) -> Result<Self, WorkerFailure> {
        Ok(Self)
    }
}

async fn bounded_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<Option<Vec<u8>>, WorkerFailure> {
    let mut result = Vec::new();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return Ok(if result.is_empty() {
                None
            } else {
                Some(result)
            });
        }
        let end = available.iter().position(|b| *b == b'\n').map(|p| p + 1);
        let count = end.unwrap_or(available.len());
        if result.len() + count > MAX_EVENT {
            return Err(WorkerFailure::new(
                "protocol",
                "Evento del motore troppo grande.",
            ));
        }
        result.extend_from_slice(&available[..count]);
        reader.consume(count);
        if end.is_some() {
            return Ok(Some(result));
        }
    }
}

pub async fn run_worker(
    runtime: RuntimePaths,
    job_id: String,
    attempt_id: String,
    command: &str,
    mut payload: Value,
    mut cancel: watch::Receiver<bool>,
    timeout: Duration,
    on_event: impl Fn(Value) + Send,
) -> Result<Value, WorkerFailure> {
    runtime
        .verify()
        .map_err(|e| WorkerFailure::new("runtime", &e))?;
    if *cancel.borrow() {
        return Err(WorkerFailure::new("cancelled", "Operazione annullata."));
    }
    payload["bin"] = json!(runtime.bin);
    let input = serde_json::to_vec(&json!({"protocol": 1, "job_id": job_id, "attempt_id": attempt_id, "command": command, "payload": payload}))
        .map_err(|_| WorkerFailure::new("protocol", "Richiesta non valida."))?;
    if input.len() >= 1024 * 1024 {
        return Err(WorkerFailure::new("protocol", "Richiesta troppo grande."));
    }
    let mut process = Command::new(&runtime.python);
    process
        .args(["-I", "-u"])
        .arg(&runtime.worker)
        .env_remove("PYTHONPATH")
        .env_remove("PYTHONHOME")
        .env("PYTHONNOUSERSITE", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .env("PIP_CONFIG_FILE", "NUL")
        .env("PATH", &runtime.bin)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    process.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    let mut child = process.spawn()?;
    // The worker waits for stdin before it can start any child processes.
    let tree = ProcessTree::assign(
        child
            .id()
            .ok_or_else(|| WorkerFailure::new("runtime", "Processo non avviato."))?,
    )?;
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(&input).await?;
    stdin.write_all(b"\n").await?;
    drop(stdin);
    let stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    // Drain continuously without retaining diagnostics or leaking signed URLs.
    let drain = tokio::spawn(async move {
        let mut buffer = [0_u8; 4096];
        loop {
            match stderr.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                _ => {}
            }
        }
    });
    let work = async {
        let mut reader = BufReader::new(stdout);
        let mut hello = false;
        let mut terminal: Option<Result<Value, WorkerFailure>> = None;
        while let Some(line) = bounded_line(&mut reader).await? {
            let event: Value = serde_json::from_slice(&line).map_err(|_| {
                WorkerFailure::new("protocol", "Evento JSON del motore non valido.")
            })?;
            if event["protocol"] != 1
                || event["job_id"] != job_id
                || event["attempt_id"] != attempt_id
            {
                return Err(WorkerFailure::new(
                    "protocol",
                    "Identificatori del motore non corrispondenti.",
                ));
            }
            let name = event["event"].as_str().unwrap_or("");
            if terminal.is_some() {
                return Err(WorkerFailure::new(
                    "protocol",
                    "Evento dopo la risposta terminale.",
                ));
            }
            if name == "hello" {
                if hello {
                    return Err(WorkerFailure::new("protocol", "Handshake duplicato."));
                }
                hello = true;
            } else if !hello {
                return Err(WorkerFailure::new(
                    "protocol",
                    "Handshake del motore mancante.",
                ));
            }
            match name {
                "result" => terminal = Some(Ok(event["data"].clone())),
                "error" => {
                    terminal = Some(Err(serde_json::from_value(event["data"].clone()).map_err(
                        |_| WorkerFailure::new("protocol", "Errore del motore non valido."),
                    )?))
                }
                "hello" | "progress" | "item" | "phase" => on_event(event),
                _ => {
                    return Err(WorkerFailure::new(
                        "protocol",
                        "Tipo evento non supportato.",
                    ))
                }
            }
        }
        let status = child.wait().await?;
        match terminal {
            Some(Ok(value)) if status.success() => Ok(value),
            Some(Err(error)) => Err(error),
            _ => Err(WorkerFailure::new(
                "engine",
                "Il motore è terminato senza un risultato valido.",
            )),
        }
    };
    let result = tokio::select! {
        value = work => value,
        _ = async { loop { if cancel.changed().await.is_err() { std::future::pending::<()>().await; } if *cancel.borrow() { break; } } } => Err(WorkerFailure::new("cancelled", "Operazione annullata.")),
        _ = tokio::time::sleep(timeout) => Err(WorkerFailure::new("network", "Tempo massimo dell'operazione superato.")),
    };
    drop(tree);
    let _ = child.kill().await;
    let _ = child.wait().await;
    drain.abort();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::{Arc, Mutex},
    };

    fn runtime() -> RuntimePaths {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_owned();
        let resources = std::env::var_os("FRAME_TEST_RESOURCES")
            .map(PathBuf::from)
            .unwrap_or(root.join("resources"));
        RuntimePaths::bundled(&resources).unwrap()
    }

    #[tokio::test]
    async fn private_worker_roundtrip_and_missing_runtime() {
        let (_tx, rx) = watch::channel(false);
        let paths = runtime();
        let result = run_worker(
            paths.clone(),
            "test".into(),
            "one".into(),
            "healthcheck",
            json!({}),
            rx,
            Duration::from_secs(15),
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(result["protocol"], 1);
        let mut missing = paths;
        missing.bin = std::env::temp_dir().join("frame-missing-native-tools");
        assert!(missing.verify().is_err());
    }

    #[tokio::test]
    async fn oversized_line_is_rejected() {
        let data = vec![b'x'; MAX_EVENT + 1];
        assert_eq!(
            bounded_line(&mut BufReader::new(&data[..]))
                .await
                .unwrap_err()
                .code,
            "protocol"
        );
    }

    #[tokio::test]
    async fn timeout_and_abrupt_exit_are_failures() {
        let temporary = tempfile::tempdir().unwrap();
        for (source, expected) in [
            (
                "import sys,time; sys.stdin.readline(); time.sleep(120)",
                "network",
            ),
            ("import sys; sys.stdin.readline(); sys.exit(3)", "engine"),
        ] {
            let script = temporary.path().join("failing.py");
            fs::write(&script, source).unwrap();
            let mut paths = runtime();
            paths.worker = script;
            let (_tx, rx) = watch::channel(false);
            let result = run_worker(
                paths,
                "test".into(),
                "failure".into(),
                "healthcheck",
                json!({}),
                rx,
                Duration::from_millis(800),
                |_| {},
            )
            .await;
            assert_eq!(result.unwrap_err().code, expected);
        }
    }

    #[tokio::test]
    async fn cancel_kills_grandchild() {
        let temporary = tempfile::tempdir().unwrap();
        let script = temporary.path().join("waiting.py");
        fs::write(&script, r#"import json, subprocess, sys, time
r=json.loads(sys.stdin.readline())
c=subprocess.Popen([CHILD_EXE, '-re', '-f', 'lavfi', '-i', 'sine=frequency=440', '-f', 'null', '-'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
for event,data in [('hello',{}),('progress',{'pid':c.pid})]:
 print(json.dumps(dict(protocol=1,job_id=r['job_id'],attempt_id=r['attempt_id'],event=event,data=data)),flush=True)
time.sleep(120)
"#.replace("CHILD_EXE",&serde_json::to_string(&runtime().bin.join("ffmpeg.exe").to_string_lossy()).unwrap())).unwrap();
        let mut paths = runtime();
        paths.worker = script;
        let (tx, rx) = watch::channel(false);
        let pid = Arc::new(Mutex::new(None));
        let saved = pid.clone();
        let task = tokio::spawn(run_worker(
            paths,
            "test".into(),
            "cancel".into(),
            "healthcheck",
            json!({}),
            rx,
            Duration::from_secs(20),
            move |e| {
                if let Some(value) = e["data"]["pid"].as_u64() {
                    *saved.lock().unwrap() = Some(value as u32);
                }
            },
        ));
        for _ in 0..100 {
            if pid.lock().unwrap().is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let child_pid = pid.lock().unwrap().expect("grandchild started");
        tx.send(true).unwrap();
        assert_eq!(task.await.unwrap().unwrap_err().code, "cancelled");
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::{
                Foundation::{CloseHandle, WAIT_TIMEOUT},
                System::Threading::*,
            };
            let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, child_pid);
            if !process.is_null() {
                assert_ne!(WaitForSingleObject(process, 3000), WAIT_TIMEOUT);
                CloseHandle(process);
            }
        }
    }
}
