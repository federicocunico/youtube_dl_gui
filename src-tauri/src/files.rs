use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub fn filename(value: &str) -> String {
    let value: String = value
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(120)
        .collect();
    let mut name = value
        .trim()
        .trim_matches(|c| c == '.' || c == ' ')
        .to_string();
    if name.is_empty() {
        name = "Video".into();
    }
    let base = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_ascii_uppercase();
    if [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9", "COM¹",
        "COM²", "COM³", "LPT¹", "LPT²", "LPT³",
    ]
    .contains(&base.as_str())
    {
        name.insert(0, '_');
    }
    for extension in [
        ".mp4", ".mkv", ".webm", ".m4a", ".mp3", ".aac", ".opus", ".ogg", ".wav", ".flac",
    ] {
        if name.to_ascii_lowercase().ends_with(extension) {
            name.truncate(name.len() - extension.len());
            break;
        }
    }
    if name.is_empty() {
        "Video".into()
    } else {
        name.trim_end_matches(|c| c == ' ' || c == '.').to_string()
    }
}

pub fn destination(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);
    if !path.is_absolute() || !path.is_dir() {
        return Err("Scegli una cartella di destinazione esistente.".into());
    }
    let directory = path
        .canonicalize()
        .map_err(|_| "Cartella di destinazione non accessibile.")?;
    let probe = directory.join(format!(".frame-write-{}", uuid::Uuid::new_v4()));
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(|_| "La cartella non è scrivibile.")?;
    fs::remove_file(probe)
        .map_err(|_| "Impossibile pulire il file di verifica nella destinazione.")?;
    Ok(directory)
}

pub fn candidate(
    directory: &Path,
    name: &str,
    extension: &str,
    index: u32,
) -> Result<PathBuf, String> {
    if ![
        "mp4", "mkv", "webm", "m4a", "mp3", "aac", "opus", "ogg", "wav", "flac",
    ]
    .contains(&extension)
    {
        return Err("Estensione non supportata.".into());
    }
    let stem = filename(name);
    let name = if index == 0 {
        format!("{stem}.{extension}")
    } else {
        format!("{stem} ({index}).{extension}")
    };
    Ok(directory.join(name))
}

pub fn move_exclusive(source: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let source: Vec<_> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let destination: Vec<_> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::MoveFileW(
                source.as_ptr(),
                destination.as_ptr(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        fs::hard_link(source, destination)?;
        fs::remove_file(source)
    }
}

pub fn clean_temporary(directory: &Path, destination: &Path, job_id: &str) -> Result<(), String> {
    if !directory.exists() {
        return Ok(());
    }
    let actual = directory.canonicalize().map_err(|e| e.to_string())?;
    let expected = destination.canonicalize().map_err(|e| e.to_string())?;
    if actual.parent() != Some(expected.as_path())
        || !actual
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .starts_with(&format!(".frame-{job_id}-"))
        || fs::symlink_metadata(directory)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
    {
        return Err("Directory temporanea fuori dalla destinazione prevista.".into());
    }
    fs::remove_dir_all(actual).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_publication_has_one_winner_and_missing_folder_is_actionable() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("same.mp4");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles: Vec<_> = (0..2)
            .map(|index| {
                let source = temp.path().join(format!("source-{index}"));
                fs::write(&source, format!("content-{index}")).unwrap();
                let target = target.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    move_exclusive(&source, &target).is_ok()
                })
            })
            .collect();
        let wins = handles
            .into_iter()
            .map(|h| usize::from(h.join().unwrap()))
            .sum::<usize>();
        assert_eq!(wins, 1);
        assert!(fs::read_to_string(target).unwrap().starts_with("content-"));
        let missing = temp.path().join("removed");
        fs::create_dir(&missing).unwrap();
        assert!(destination(missing.to_str().unwrap()).is_ok());
        fs::remove_dir(&missing).unwrap();
        assert!(destination(missing.to_str().unwrap())
            .unwrap_err()
            .contains("esistente"));
    }
    #[test]
    fn names_are_safe_and_move_never_overwrites() {
        assert_eq!(filename("CON.mp4"), "_CON");
        assert_eq!(filename("COM¹.mp4"), "_COM¹");
        assert_eq!(filename("name. . "), "name");
        assert_eq!(filename("../hello:*?"), "_hello___");
        let temp = tempfile::tempdir().unwrap();
        let a = temp.path().join("a");
        let b = temp.path().join("b");
        fs::write(&a, b"new").unwrap();
        fs::write(&b, b"existing").unwrap();
        assert!(move_exclusive(&a, &b).is_err());
        assert_eq!(fs::read(&b).unwrap(), b"existing");
        let c = candidate(temp.path(), "../video.mp4", "mp4", 1).unwrap();
        assert_eq!(c.parent().unwrap(), temp.path());
        move_exclusive(&a, &c).unwrap();
        assert_eq!(fs::read(c).unwrap(), b"new");
    }
}
