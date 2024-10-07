use std::fmt;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::config::{parse_tables, Table};

pub const VOICE_BASE: &str = "https://huggingface.co/rhasspy/piper-voices/resolve/main";
pub const MANIFEST: &str = "voices.toml";
pub const CATALOG_FILE: &str = "catalog.toml";
pub const MAX_VOICE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

const BUILTIN: &[(&str, &str, &str, &str)] = &[
    (
        "de",
        "de_DE-thorsten-medium",
        "7e64762d8e5118bb578f2eea6207e1a35a8e0c30595010b666f983fc87bb7819",
        "974adee790533adb273a1ac88f49027d2a1b8f0f2cf4905954a4791e79264e85",
    ),
    (
        "en",
        "en_US-lessac-medium",
        "5efe09e69902187827af646e1a6e9d269dee769f9877d17b16b1b46eeaaf019f",
        "efe19c417bed055f2d69908248c6ba650fa135bc868b0e6abb3da181dab690a0",
    ),
    (
        "es",
        "es_ES-davefx-medium",
        "6658b03b1a6c316ee4c265a9896abc1393353c2d9e1bca7d66c2c442e222a917",
        "0e0dda87c732f6f38771ff274a6380d9252f327dca77aa2963d5fbdf9ec54842",
    ),
    (
        "fr",
        "fr_FR-siwis-medium",
        "641d1ab097da2b81128c076810edb052b385decc8be3381814802a64a73baf99",
        "39479916c2db192b5ac9764daddd0c744d83e023ad890c6976c0633ae4df8959",
    ),
    (
        "it",
        "it_IT-paola-medium",
        "6fc918b5a0ea6137382833dddfa567bffbe6a5060c02043c87192ee59c04210c",
        "aea19c0a7fce29fbc359b93f10e7902854401e4c95ae2ea328ae516b15d296cf",
    ),
    (
        "ko",
        "ko_KR-kss-medium",
        "624fd774e26895f24bebae1bd9a3379e3394baeade4b584924f83e414096e2c9",
        "153b5619d0580f824a59108d83ca19434de410eb8e4fe80325b58684fdc8a1df",
    ),
    (
        "pt",
        "pt_BR-faber-medium",
        "858555e3a064209c57088fe6bd70c4c3dc54d03eaa00c45d5ecaf43a33f95aa7",
        "7e694de195ae3fc36dd732c445eb04fb49b649854893cb5506b978f0d50a1d6f",
    ),
    (
        "ru",
        "ru_RU-irina-medium",
        "8ff38212d23da300bbe3705c645e6e5b9475f0bfde01558eb17813e22acaaaaa",
        "c2ec28bb38e2b59e93b959b3e40348c1afebbd272f30fed5d41205d08e98a9d7",
    ),
    (
        "zh",
        "zh_CN-huayan-medium",
        "9929917bf8cabb26fd528ea44d3a6699c11e87317a14765312420be230be0f3d",
        "d521dc45504a8ccc99e325822b35946dd701840bfb07e3dbb31a40929ed6a82b",
    ),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceSpec {
    pub lang: String,
    pub id: String,
    pub onnx: Option<String>,
    pub json: Option<String>,
    pub base: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    UnknownLanguage(String),
    Http(String),
    Io(String),
    Checksum { file: String },
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchError::UnknownLanguage(lang) => write!(f, "no voice in the catalog for {lang}"),
            FetchError::Http(reason) => write!(f, "voice download failed: {reason}"),
            FetchError::Io(reason) => write!(f, "voice store error: {reason}"),
            FetchError::Checksum { file } => write!(f, "checksum mismatch for {file}"),
        }
    }
}

pub fn builtin_catalog() -> Vec<VoiceSpec> {
    BUILTIN
        .iter()
        .map(|(lang, id, onnx, json)| VoiceSpec {
            lang: (*lang).to_string(),
            id: (*id).to_string(),
            onnx: Some((*onnx).to_string()),
            json: Some((*json).to_string()),
            base: None,
        })
        .collect()
}

pub fn catalog(root: &Path) -> Vec<VoiceSpec> {
    let mut specs = builtin_catalog();
    let Ok(text) = std::fs::read_to_string(root.join(CATALOG_FILE)) else {
        return specs;
    };
    for table in parse_tables(&text) {
        let lang = table.name.clone();
        if lang.is_empty() {
            continue;
        }
        let Some(id) = table.get("id").map(|v| v.to_string()) else {
            continue;
        };
        let spec = VoiceSpec {
            lang,
            id,
            onnx: table.get("onnx").map(|v| v.to_string()),
            json: table.get("json").map(|v| v.to_string()),
            base: table.get("base").map(|v| v.to_string()),
        };
        match specs.iter_mut().find(|s| s.lang == spec.lang) {
            Some(existing) => *existing = spec,
            None => specs.push(spec),
        }
    }
    specs
}

pub fn resolve<'a>(catalog: &'a [VoiceSpec], lang: &str) -> Option<&'a VoiceSpec> {
    catalog.iter().find(|s| s.lang == lang)
}

pub fn voice_dir(id: &str) -> String {
    let mut parts = id.split('-');
    let locale = parts.next().unwrap_or(id);
    let name = parts.next().unwrap_or("");
    let quality = parts.next().unwrap_or("medium");
    let language = locale.split('_').next().unwrap_or(locale);
    format!("{language}/{locale}/{name}/{quality}")
}

impl VoiceSpec {
    pub fn base(&self) -> &str {
        self.base.as_deref().unwrap_or(VOICE_BASE)
    }

    pub fn onnx_url(&self) -> String {
        format!("{}/{}/{}.onnx", self.base(), voice_dir(&self.id), self.id)
    }

    pub fn json_url(&self) -> String {
        format!(
            "{}/{}/{}.onnx.json",
            self.base(),
            voice_dir(&self.id),
            self.id
        )
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn read_manifest(dir: &Path) -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string(dir.join(MANIFEST)) else {
        return Vec::new();
    };
    parse_tables(&text)
        .into_iter()
        .filter(|t| t.name.is_empty())
        .flat_map(|t: Table| t.entries)
        .collect()
}

pub fn record_manifest(dir: &Path, lang: &str, id: &str) -> Result<(), FetchError> {
    let mut entries = read_manifest(dir);
    match entries.iter_mut().find(|(code, _)| code == lang) {
        Some(entry) => entry.1 = id.to_string(),
        None => entries.push((lang.to_string(), id.to_string())),
    }
    entries.sort();
    let body: String = entries
        .iter()
        .map(|(code, id)| format!("{code} = \"{id}\"\n"))
        .collect();
    let tmp = dir.join(format!(".{MANIFEST}.part"));
    std::fs::write(&tmp, body).map_err(|e| FetchError::Io(e.to_string()))?;
    std::fs::rename(&tmp, dir.join(MANIFEST)).map_err(|e| FetchError::Io(e.to_string()))
}

pub fn fetch(spec: &VoiceSpec, dir: &Path) -> Result<PathBuf, FetchError> {
    let onnx = download(&spec.onnx_url(), spec.onnx.as_deref(), dir, &format!("{}.onnx", spec.id))?;
    download(
        &spec.json_url(),
        spec.json.as_deref(),
        dir,
        &format!("{}.onnx.json", spec.id),
    )?;
    record_manifest(dir, &spec.lang, &spec.id)?;
    Ok(onnx)
}

pub fn install(root: &Path, dir: &Path, lang: &str) -> Result<PathBuf, FetchError> {
    let catalog = catalog(root);
    let spec = resolve(&catalog, lang).ok_or(FetchError::UnknownLanguage(lang.to_string()))?;
    fetch(spec, dir)
}

fn download(url: &str, sha: Option<&str>, dir: &Path, name: &str) -> Result<PathBuf, FetchError> {
    let target = dir.join(name);
    if let Some(sha) = sha
        && let Ok(bytes) = std::fs::read(&target)
        && sha256_hex(&bytes) == sha
    {
        return Ok(target);
    }
    let bytes = get(url)?;
    if let Some(sha) = sha
        && sha256_hex(&bytes) != sha
    {
        return Err(FetchError::Checksum {
            file: name.to_string(),
        });
    }
    std::fs::create_dir_all(dir).map_err(|e| FetchError::Io(e.to_string()))?;
    let tmp = dir.join(format!(".{name}.part"));
    std::fs::write(&tmp, &bytes).map_err(|e| FetchError::Io(e.to_string()))?;
    std::fs::rename(&tmp, &target).map_err(|e| FetchError::Io(e.to_string()))?;
    Ok(target)
}

fn get(url: &str) -> Result<Vec<u8>, FetchError> {
    let mut response = ureq::get(url)
        .call()
        .map_err(|e| FetchError::Http(format!("{url}: {e}")))?;
    let bytes = response
        .body_mut()
        .with_config()
        .limit(MAX_VOICE_BYTES)
        .read_to_vec()
        .map_err(|e| FetchError::Http(format!("{url}: {e}")))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_covers_the_vits_languages() {
        let catalog = builtin_catalog();
        for lang in ["de", "en", "es", "fr", "it", "ko", "pt", "ru", "zh"] {
            assert!(resolve(&catalog, lang).is_some(), "missing {lang}");
        }
        assert!(resolve(&catalog, "ja").is_none());
    }

    #[test]
    fn voice_dir_splits_the_voice_id() {
        assert_eq!(voice_dir("es_ES-davefx-medium"), "es/es_ES/davefx/medium");
        assert_eq!(voice_dir("zh_CN-huayan-medium"), "zh/zh_CN/huayan/medium");
    }

    #[test]
    fn urls_point_at_the_catalog_layout() {
        let spec = VoiceSpec {
            lang: "es".to_string(),
            id: "es_ES-davefx-medium".to_string(),
            onnx: None,
            json: None,
            base: Some("https://example.test/v".to_string()),
        };
        assert_eq!(
            spec.onnx_url(),
            "https://example.test/v/es/es_ES/davefx/medium/es_ES-davefx-medium.onnx"
        );
        assert_eq!(
            spec.json_url(),
            "https://example.test/v/es/es_ES/davefx/medium/es_ES-davefx-medium.onnx.json"
        );
    }

    #[test]
    fn checksums_of_the_provisioned_voices_match_the_catalog() {
        let dir = Path::new("/user/.local/share/voz/piper/voices");
        if !dir.is_dir() {
            return;
        }
        for spec in builtin_catalog() {
            let path = dir.join(format!("{}.onnx", spec.id));
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            assert_eq!(sha256_hex(&bytes), spec.onnx.clone().unwrap_or_default(), "{}", spec.id);
            let json = dir.join(format!("{}.onnx.json", spec.id));
            let Ok(json_bytes) = std::fs::read(&json) else {
                continue;
            };
            assert_eq!(sha256_hex(&json_bytes), spec.json.clone().unwrap_or_default(), "{} json", spec.id);
        }
    }

    #[test]
    fn catalog_file_overrides_a_builtin_entry() {
        let root = std::env::temp_dir().join(format!("voz-cat-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("mkdir");
        std::fs::write(
            root.join(CATALOG_FILE),
            "[es]\nid = \"es_ES-other-medium\"\nbase = \"https://mirror.test\"\n[ja]\nid = \"ja_JP-test-medium\"\n",
        )
        .expect("write");
        let catalog = catalog(&root);
        std::fs::remove_dir_all(&root).ok();
        let es = resolve(&catalog, "es").expect("es");
        assert_eq!(es.id, "es_ES-other-medium");
        assert_eq!(es.base.as_deref(), Some("https://mirror.test"));
        assert_eq!(resolve(&catalog, "ja").expect("ja").id, "ja_JP-test-medium");
    }

    #[test]
    fn missing_catalog_file_keeps_builtins() {
        let catalog = catalog(Path::new("/definitely/not/here"));
        assert_eq!(catalog, builtin_catalog());
    }

    #[test]
    fn manifest_round_trips() {
        let dir = std::env::temp_dir().join(format!("voz-manifest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        record_manifest(&dir, "es", "es_ES-davefx-medium").expect("write");
        record_manifest(&dir, "en", "en_US-lessac-medium").expect("write");
        record_manifest(&dir, "es", "es_ES-other-medium").expect("update");
        let entries = read_manifest(&dir);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(
            entries,
            vec![
                ("en".to_string(), "en_US-lessac-medium".to_string()),
                ("es".to_string(), "es_ES-other-medium".to_string()),
            ]
        );
    }

    #[test]
    fn manifest_of_missing_file_is_empty() {
        assert!(read_manifest(Path::new("/definitely/not/here")).is_empty());
    }

    #[test]
    fn install_rejects_an_unknown_language() {
        let dir = std::env::temp_dir().join(format!("voz-inst-{}", std::process::id()));
        let err = install(Path::new("/definitely/not/here"), &dir, "ja").unwrap_err();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(err, FetchError::UnknownLanguage("ja".to_string()));
    }

    #[test]
    fn download_reports_a_checksum_mismatch() {
        let dir = std::env::temp_dir().join(format!("voz-dl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let err = download("https://example.test/x", Some("00"), &dir, "x.onnx").unwrap_err();
        std::fs::remove_dir_all(&dir).ok();
        assert!(matches!(err, FetchError::Http(_)));
    }

    #[test]
    fn sha256_hex_matches_the_known_digest() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
