use regex::Regex;
use serde::Serialize;
use std::collections::HashSet;
use url::Url;

#[derive(Debug, Clone, Serialize)]
pub struct MediaUrl {
    pub url: String,
    pub identity: String,
    pub playlist: bool,
}

pub fn normalize(input: &str) -> Result<MediaUrl, String> {
    let input = input.trim();
    if input.len() > 8192 {
        return Err("URL troppo lungo.".into());
    }
    let value = if input.contains("://") {
        input.to_string()
    } else {
        format!("https://{input}")
    };
    let url = Url::parse(&value).map_err(|_| "Inserisci un URL YouTube valido.")?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some_and(|p| p != 443)
    {
        return Err(
            "Usa un collegamento HTTPS YouTube senza credenziali o porte personalizzate.".into(),
        );
    }
    let host = url.host_str().unwrap_or("");
    if ![
        "youtube.com",
        "www.youtube.com",
        "m.youtube.com",
        "youtu.be",
    ]
    .contains(&host)
    {
        return Err("Sono supportati solo collegamenti YouTube.".into());
    }
    let pairs: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    let video = if host == "youtu.be" {
        Some(url.path().trim_matches('/').to_string())
    } else if url.path() == "/watch" {
        pairs.get("v").cloned()
    } else {
        url.path()
            .strip_prefix("/shorts/")
            .map(|v| v.trim_end_matches('/').to_string())
    };
    if let Some(id) = video {
        if !Regex::new(r"^[A-Za-z0-9_-]{11}$").unwrap().is_match(&id) {
            return Err("Identificatore video non valido.".into());
        }
        return Ok(MediaUrl {
            url: format!("https://www.youtube.com/watch?v={id}"),
            identity: id,
            playlist: false,
        });
    }
    if url.path() == "/playlist" {
        let id = pairs
            .get("list")
            .ok_or("Identificatore playlist mancante.")?;
        if !Regex::new(r"^[A-Za-z0-9_-]{10,150}$").unwrap().is_match(id) {
            return Err("Identificatore playlist non valido.".into());
        }
        return Ok(MediaUrl {
            url: format!("https://www.youtube.com/playlist?list={id}"),
            identity: id.clone(),
            playlist: true,
        });
    }
    Err("Incolla l'URL di un video o di una playlist, non la homepage o un canale.".into())
}

pub fn import_urls(input: &str) -> Vec<(String, Result<MediaUrl, String>, bool)> {
    let mut seen = HashSet::new();
    input
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let normalized = normalize(line);
            let duplicate = normalized
                .as_ref()
                .is_ok_and(|v| !seen.insert(v.identity.clone()));
            (line.to_string(), normalized, duplicate)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supported_forms_and_deduplication() {
        for url in [
            "https://youtu.be/BaW_jenozKc?t=5",
            "youtube.com/watch?v=BaW_jenozKc&list=PL123456789",
            "https://m.youtube.com/shorts/BaW_jenozKc",
        ] {
            assert_eq!(normalize(url).unwrap().identity, "BaW_jenozKc");
        }
        assert!(
            normalize("https://youtube.com/playlist?list=PL123456789")
                .unwrap()
                .playlist
        );
        let rows = import_urls(
            "https://youtu.be/BaW_jenozKc\nhttps://youtube.com/watch?v=BaW_jenozKc\ninvalid",
        );
        assert!(!rows[0].2);
        assert!(rows[1].2);
        assert!(rows[2].1.is_err());
    }
    #[test]
    fn hostile_and_nonmedia_urls_rejected() {
        for url in [
            "https://youtube.com",
            "https://youtube.com.evil.org/watch?v=BaW_jenozKc",
            "http://youtube.com/watch?v=BaW_jenozKc",
            "https://user:pass@youtube.com/watch?v=BaW_jenozKc",
            "https://youtube.com:9000/watch?v=BaW_jenozKc",
            "https://youtube.com/@channel",
            "https://youtu.be/../file",
            "https://youtu.be/wrong",
        ] {
            assert!(normalize(url).is_err(), "{url}");
        }
    }
}
