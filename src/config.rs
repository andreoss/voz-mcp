use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub name: String,
    pub entries: Vec<(String, String)>,
}

impl Table {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

pub fn parse_tables(text: &str) -> Vec<Table> {
    let mut tables: Vec<Table> = Vec::new();
    let mut name = String::new();
    let mut entries: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            if !name.is_empty() || !entries.is_empty() {
                tables.push(Table {
                    name: std::mem::take(&mut name),
                    entries: std::mem::take(&mut entries),
                });
            }
            name = header.trim().to_string();
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            entries.push((key.trim().to_string(), unquote(value.trim())));
        }
    }
    if !name.is_empty() || !entries.is_empty() {
        tables.push(Table { name, entries });
    }
    tables
}

pub fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        return value[1..value.len() - 1].to_string();
    }
    value.to_string()
}

pub fn parse_bool(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    pub bin: Option<PathBuf>,
    pub voices: Option<PathBuf>,
    pub auto_fetch: Option<bool>,
    pub voice: Vec<(String, String)>,
}

pub fn load(path: &Path) -> Config {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Config::default();
    };
    let mut config = Config::default();
    for table in parse_tables(&text) {
        match table.name.as_str() {
            "piper" => {
                config.bin = table.get("bin").map(PathBuf::from);
                config.voices = table.get("voices").map(PathBuf::from);
                config.auto_fetch = table.get("auto_fetch").and_then(parse_bool);
            }
            "voice" => config.voice = table.entries.clone(),
            "" if config.auto_fetch.is_none() => {
                config.auto_fetch = table.get("auto_fetch").and_then(parse_bool);
            }
            _ => {}
        }
    }
    config
}

pub fn config_path(root: &Path) -> PathBuf {
    match std::env::var_os("VOZ_CONFIG") {
        Some(path) => PathBuf::from(path),
        None => root.join("voz.toml"),
    }
}

pub fn auto_fetch(config: &Config) -> bool {
    match std::env::var_os("VOZ_AUTO_FETCH") {
        Some(value) => parse_bool(&value.to_string_lossy()).unwrap_or(true),
        None => config.auto_fetch.unwrap_or(true),
    }
}

pub fn voice_choice(config: &Config, lang: &str, manifest: &[(String, String)]) -> Option<String> {
    config
        .voice
        .iter()
        .chain(manifest.iter())
        .find(|(code, _)| code == lang)
        .map(|(_, id)| id.clone())
}

pub fn preferred_voices(config: &Config, manifest: &[(String, String)]) -> Vec<(String, String)> {
    let mut voices = manifest.to_vec();
    for (lang, id) in &config.voice {
        match voices.iter_mut().find(|(code, _)| code == lang) {
            Some(entry) => entry.1 = id.clone(),
            None => voices.push((lang.clone(), id.clone())),
        }
    }
    voices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sections_and_values() {
        let tables = parse_tables("[piper]\nbin = \"/x/piper\"\nvoices = '/x/voices'\n[voice]\nes = \"es_ES-davefx-medium\"\n");
        assert_eq!(tables.len(), 2);
        assert_eq!(tables[0].name, "piper");
        assert_eq!(tables[0].get("bin"), Some("/x/piper"));
        assert_eq!(tables[0].get("voices"), Some("/x/voices"));
        assert_eq!(tables[1].name, "voice");
        assert_eq!(tables[1].get("es"), Some("es_ES-davefx-medium"));
    }

    #[test]
    fn parses_top_level_entries() {
        let tables = parse_tables("auto_fetch = false\n# comment\n\n");
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].name, "");
        assert_eq!(tables[0].get("auto_fetch"), Some("false"));
    }

    #[test]
    fn unknown_key_is_absent() {
        let tables = parse_tables("[piper]\nbin = \"x\"\n");
        assert_eq!(tables[0].get("voices"), None);
    }

    #[test]
    fn load_reads_every_section() {
        let dir = std::env::temp_dir().join(format!("voz-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("voz.toml");
        std::fs::write(&path, "[piper]\nbin = \"/b\"\nvoices = \"/v\"\nauto_fetch = false\n[voice]\nes = \"es_ES-a-medium\"\n").expect("write");
        let config = load(&path);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(config.bin, Some(PathBuf::from("/b")));
        assert_eq!(config.voices, Some(PathBuf::from("/v")));
        assert_eq!(config.auto_fetch, Some(false));
        assert_eq!(
            config.voice,
            vec![("es".to_string(), "es_ES-a-medium".to_string())]
        );
    }

    #[test]
    fn load_of_missing_file_is_default() {
        let config = load(Path::new("/definitely/not/here/voz.toml"));
        assert_eq!(config, Config::default());
    }

    #[test]
    fn parses_every_bool_spelling() {
        assert_eq!(parse_bool("true"), Some(true));
        assert_eq!(parse_bool("YES"), Some(true));
        assert_eq!(parse_bool("1"), Some(true));
        assert_eq!(parse_bool("off"), Some(false));
        assert_eq!(parse_bool("0"), Some(false));
        assert_eq!(parse_bool("maybe"), None);
    }

    #[test]
    fn auto_fetch_defaults_to_enabled() {
        assert!(auto_fetch(&Config::default()));
        assert!(!auto_fetch(&Config {
            auto_fetch: Some(false),
            ..Config::default()
        }));
    }

    #[test]
    fn config_path_defaults_below_root() {
        assert_eq!(config_path(Path::new("/r")), PathBuf::from("/r/voz.toml"));
    }

    #[test]
    fn voice_choice_prefers_config_over_manifest() {
        let config = Config {
            voice: vec![("es".to_string(), "es_ES-from-config".to_string())],
            ..Config::default()
        };
        let manifest = vec![("es".to_string(), "es_ES-from-manifest".to_string())];
        assert_eq!(
            voice_choice(&config, "es", &manifest),
            Some("es_ES-from-config".to_string())
        );
        assert_eq!(voice_choice(&Config::default(), "es", &manifest), Some("es_ES-from-manifest".to_string()));
        assert_eq!(voice_choice(&Config::default(), "ja", &manifest), None);
    }

    #[test]
    fn preferred_voices_merges_config_over_manifest() {
        let config = Config {
            voice: vec![
                ("es".to_string(), "es_ES-from-config".to_string()),
                ("ja".to_string(), "ja_JP-test".to_string()),
            ],
            ..Config::default()
        };
        let manifest = vec![("es".to_string(), "es_ES-from-manifest".to_string())];
        assert_eq!(
            preferred_voices(&config, &manifest),
            vec![
                ("es".to_string(), "es_ES-from-config".to_string()),
                ("ja".to_string(), "ja_JP-test".to_string()),
            ]
        );
    }
}
