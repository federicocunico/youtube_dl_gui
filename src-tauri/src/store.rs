use crate::model::{now, Job, Settings};
use fs2::FileExt;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    path::Path,
};

pub struct Store {
    connection: Connection,
    _instance_lock: File,
}

fn db_error(error: impl std::fmt::Display) -> String {
    format!("Errore archivio locale: {error}")
}

impl Store {
    pub fn open(directory: &Path) -> Result<Self, String> {
        fs::create_dir_all(directory).map_err(db_error)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("instance.lock"))
            .map_err(db_error)?;
        lock.try_lock_exclusive()
            .map_err(|_| "Frame è già aperto. Usa la finestra esistente.".to_string())?;
        let connection = Connection::open(directory.join("state.sqlite")).map_err(db_error)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(db_error)?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;",
            )
            .map_err(db_error)?;
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(db_error)?;
        if version > 1 {
            return Err("Archivio creato da una versione più recente di Frame.".into());
        }
        if version == 0 {
            connection.execute_batch("BEGIN IMMEDIATE;
                CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
                CREATE TABLE jobs(id TEXT PRIMARY KEY, position INTEGER NOT NULL, state TEXT NOT NULL, data TEXT NOT NULL);
                CREATE INDEX job_order ON jobs(position);
                CREATE TABLE attempts(id TEXT PRIMARY KEY, job_id TEXT NOT NULL, created_at INTEGER NOT NULL, data TEXT NOT NULL);
                CREATE TABLE environments(id TEXT PRIMARY KEY, path TEXT NOT NULL, validated INTEGER NOT NULL DEFAULT 0);
                CREATE TABLE update_history(id INTEGER PRIMARY KEY, created_at INTEGER NOT NULL, data TEXT NOT NULL);
                PRAGMA user_version=1;
                COMMIT;").map_err(db_error)?;
        }
        Ok(Self {
            connection,
            _instance_lock: lock,
        })
    }

    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, String> {
        let value: Option<String> = self
            .connection
            .query_row("SELECT value FROM meta WHERE key=?1", [key], |r| r.get(0))
            .optional()
            .map_err(db_error)?;
        value
            .map(|v| serde_json::from_str(&v).map_err(db_error))
            .transpose()
    }

    pub fn set<T: Serialize>(&self, key: &str, value: &T) -> Result<(), String> {
        self.connection.execute("INSERT INTO meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, serde_json::to_string(value).map_err(db_error)?]).map_err(db_error)?;
        Ok(())
    }

    pub fn settings(&self) -> Result<Settings, String> {
        Ok(self.get("settings")?.unwrap_or_default())
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<(), String> {
        settings.validate()?;
        self.set("settings", settings)
    }

    pub fn jobs(&self) -> Result<Vec<Job>, String> {
        let mut statement = self
            .connection
            .prepare("SELECT data FROM jobs ORDER BY position,id")
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(db_error)?;
        rows.map(|r| serde_json::from_str(&r.map_err(db_error)?).map_err(db_error))
            .collect()
    }

    pub fn save_job(&self, job: &Job) -> Result<(), String> {
        self.connection.execute("INSERT INTO jobs(id,position,state,data) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET position=excluded.position,state=excluded.state,data=excluded.data",
            params![job.id, job.position, job.state, serde_json::to_string(job).map_err(db_error)?]).map_err(db_error)?;
        Ok(())
    }

    pub fn save_jobs(&mut self, jobs: &[Job]) -> Result<(), String> {
        let transaction = self.connection.transaction().map_err(db_error)?;
        for job in jobs {
            transaction.execute("INSERT INTO jobs(id,position,state,data) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET position=excluded.position,state=excluded.state,data=excluded.data",
                params![job.id, job.position, job.state, serde_json::to_string(job).map_err(db_error)?]).map_err(db_error)?;
        }
        transaction.commit().map_err(db_error)
    }

    pub fn remove_job(&self, id: &str) -> Result<(), String> {
        self.connection
            .execute("DELETE FROM jobs WHERE id=?1", [id])
            .map_err(db_error)?;
        Ok(())
    }

    pub fn record_attempt<T: Serialize>(
        &self,
        id: &str,
        job: &str,
        data: &T,
    ) -> Result<(), String> {
        self.connection
            .execute(
                "INSERT OR REPLACE INTO attempts(id,job_id,created_at,data) VALUES(?1,?2,?3,?4)",
                params![
                    id,
                    job,
                    now() as i64,
                    serde_json::to_string(data).map_err(db_error)?
                ],
            )
            .map_err(db_error)?;
        Ok(())
    }

    pub fn history(&self) -> Result<Vec<serde_json::Value>, String> {
        let mut statement = self
            .connection
            .prepare("SELECT data FROM update_history ORDER BY id DESC LIMIT 40")
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(db_error)?;
        rows.map(|r| serde_json::from_str(&r.map_err(db_error)?).map_err(db_error))
            .collect()
    }

    pub fn promote(
        &mut self,
        active: &str,
        previous: &str,
        report: &serde_json::Value,
    ) -> Result<(), String> {
        self.commit_engine_change(active, previous, report, None, None)
    }

    pub fn commit_engine_change(
        &mut self,
        active: &str,
        previous: &str,
        report: &serde_json::Value,
        jobs: Option<&[Job]>,
        quarantine: Option<&[String]>,
    ) -> Result<(), String> {
        let transaction = self.connection.transaction().map_err(db_error)?;
        for (key, value) in [("active_engine", active), ("previous_engine", previous)] {
            transaction.execute("INSERT INTO meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, serde_json::to_string(value).map_err(db_error)?]).map_err(db_error)?;
        }
        transaction
            .execute(
                "INSERT INTO update_history(created_at,data) VALUES(?1,?2)",
                params![now() as i64, report.to_string()],
            )
            .map_err(db_error)?;
        if let Some(jobs) = jobs {
            for job in jobs {
                transaction
                    .execute(
                        "UPDATE jobs SET position=?2,state=?3,data=?4 WHERE id=?1",
                        params![
                            job.id,
                            job.position,
                            job.state,
                            serde_json::to_string(job).map_err(db_error)?
                        ],
                    )
                    .map_err(db_error)?;
            }
        }
        if let Some(quarantine) = quarantine {
            transaction.execute("INSERT INTO meta(key,value) VALUES('quarantine',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[serde_json::to_string(quarantine).map_err(db_error)?]).map_err(db_error)?;
        }
        transaction.commit().map_err(db_error)
    }

    pub fn log_update(&self, report: &serde_json::Value) -> Result<(), String> {
        self.connection
            .execute(
                "INSERT INTO update_history(created_at,data) VALUES(?1,?2)",
                params![now() as i64, report.to_string()],
            )
            .map_err(db_error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclusive_instance_and_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        assert!(Store::open(directory.path()).is_err());
        let mut settings = store.settings().unwrap();
        settings.defaults.destination = "C:\\Downloads".into();
        store.save_settings(&settings).unwrap();
        drop(store);
        let reopened = Store::open(directory.path()).unwrap();
        assert_eq!(
            reopened.settings().unwrap().defaults.destination,
            "C:\\Downloads"
        );
    }

    #[test]
    fn engine_pointers_commit_together() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path()).unwrap();
        store
            .promote("new", "old", &serde_json::json!({"state":"activated"}))
            .unwrap();
        drop(store);
        let store = Store::open(directory.path()).unwrap();
        assert_eq!(
            store.get::<String>("active_engine").unwrap().unwrap(),
            "new"
        );
        assert_eq!(
            store.get::<String>("previous_engine").unwrap().unwrap(),
            "old"
        );
        assert_eq!(store.history().unwrap().len(), 1);
    }

    #[test]
    fn failed_transaction_does_not_mutate_state() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path()).unwrap();
        store.set("probe", &"before").unwrap();
        {
            let tx = store.connection.transaction().unwrap();
            tx.execute("UPDATE meta SET value='\"after\"' WHERE key='probe'", [])
                .unwrap();
        }
        assert_eq!(store.get::<String>("probe").unwrap().unwrap(), "before");
    }
}
