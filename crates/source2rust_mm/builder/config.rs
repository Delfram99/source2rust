use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use super::BuildResult;

#[derive(Debug, Deserialize)]
pub struct BuildConfig {
    #[serde(default = "default_sdk")]
    pub sdk: String,
    #[serde(default = "default_cxx_standard")]
    pub cxx_standard: String,
    #[serde(default)]
    pub project_sources: Vec<String>,
    #[serde(default)]
    pub sdk_sources: Vec<String>,
    #[serde(default)]
    pub proto_files: Vec<String>,
    #[serde(default)]
    pub proto_optional_files: Vec<String>,
    #[serde(default)]
    pub additional_include_paths: Vec<String>,
    #[serde(default)]
    pub defines: Vec<String>,
    #[serde(default)]
    pub windows_exports: Vec<String>,
    #[serde(default)]
    pub linux_exports: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct PluginMetadata {
    pub name: String,
    pub alias: String,
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub log_tag: String,
}

impl BuildConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.cxx_standard != "c++20" {
            return Err(format!("C++ standard must be c++20, got {}", self.cxx_standard));
        }
        validate_component(&self.sdk, "SDK name")
    }
}

impl PluginMetadata {
    pub fn validate(&self) -> Result<(), String> {
        validate_component(&self.alias, "plugin alias")
    }

    pub fn log_tag(&self) -> &str {
        if self.log_tag.is_empty() { &self.name } else { &self.log_tag }
    }
}

#[derive(Debug, Deserialize)]
pub struct SdkManifest {
    pub name: String,
    pub extension: String,
    pub code: u32,
    #[serde(rename = "define")]
    pub define_name: String,
    #[serde(default)]
    pub source2: bool,
    #[serde(default)]
    pub include_paths: Vec<String>,
    pub windows: Option<PlatformSdk>,
    pub linux: Option<PlatformSdk>,
}

#[derive(Debug, Deserialize)]
pub struct PlatformSdk {
    #[serde(default)]
    pub defines: Vec<String>,
    #[serde(default)]
    pub uses_system_cxxlib: bool,
    pub protoc_path: Option<String>,
    #[serde(rename = "x86_64")]
    pub x86_64: Option<ArchitectureSdk>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ArchitectureSdk {
    #[serde(default)]
    pub libs: Vec<String>,
    #[serde(default)]
    pub postlink_libs: Vec<String>,
    #[serde(default)]
    pub dynamic_libs: Vec<String>,
}

impl SdkManifest {
    pub fn engine_defines(&self, manifests: &Path) -> BuildResult<Vec<String>> {
        let mut engines = BTreeMap::new();
        for entry in fs::read_dir(manifests)? {
            let path = entry?.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let manifest: EngineManifest = load_json(&path)?;
            if engines.insert(manifest.define_name.clone(), manifest.code).is_some() {
                return Err(format!("duplicate engine define {} in {}", manifest.define_name, path.display()).into());
            }
        }
        if engines.get(&self.define_name) != Some(&self.code) {
            return Err(format!(
                "selected SDK {} does not match the engine registry entry SE_{}={}",
                self.name, self.define_name, self.code
            )
            .into());
        }

        let mut defines = vec![format!("SOURCE_ENGINE={}", self.code)];
        defines.extend(engines.into_iter().map(|(name, code)| format!("SE_{name}={code}")));
        if self.source2 {
            defines.push("META_IS_SOURCE2".to_owned());
        }
        Ok(defines)
    }
}

#[derive(Debug, Deserialize)]
struct EngineManifest {
    code: u32,
    #[serde(rename = "define")]
    define_name: String,
}

pub fn load_json<T: DeserializeOwned>(path: &Path) -> BuildResult<T> {
    let contents = fs::read_to_string(path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    serde_json::from_str(&contents).map_err(|error| format!("failed to parse {}: {error}", path.display()).into())
}

fn validate_component(value: &str, kind: &str) -> Result<(), String> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(format!("{kind} contains unsupported characters: {value}"));
    }
    Ok(())
}

fn default_sdk() -> String {
    "cs2".to_owned()
}

fn default_cxx_standard() -> String {
    "c++20".to_owned()
}
