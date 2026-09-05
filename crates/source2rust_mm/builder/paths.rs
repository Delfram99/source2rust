use std::env;
use std::path::{Path, PathBuf};

use super::BuildResult;

#[derive(Debug)]
pub struct ProjectPaths {
    pub root: PathBuf,
    pub crate_root: PathBuf,
    pub out_dir: PathBuf,
    pub cpp_dir: PathBuf,
    pub sdk: PathBuf,
    pub metamod: PathBuf,
    pub khook_include: PathBuf,
    pub manifests: PathBuf,
    pub manifest: PathBuf,
    pub build_config: PathBuf,
    pub plugin_metadata: PathBuf,
    pub generated: PathBuf,
    pub protobuf: PathBuf,
}

impl ProjectPaths {
    pub fn discover(sdk_name: &str) -> BuildResult<Self> {
        let crate_root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is not set")?);
        let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
        let root = crate_root
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| format!("cannot resolve workspace root from {}", crate_root.display()))?
            .to_path_buf();
        let external = root.join("external");
        let sdk = external.join(format!("hl2sdk-{sdk_name}"));
        let metamod = external.join("metamod-source");
        let khook_include = external.join("khook/include");
        let manifests = external.join("hl2sdk-manifests/manifests");

        let paths = Self {
            root,
            crate_root: crate_root.clone(),
            out_dir: out_dir.clone(),
            cpp_dir: crate_root.join("cpp"),
            sdk,
            metamod,
            khook_include,
            manifest: manifests.join(format!("{sdk_name}.json")),
            manifests,
            build_config: crate_root.join("build_config.json"),
            plugin_metadata: crate_root.join("plugin-metadata.json"),
            generated: out_dir.join("generated"),
            protobuf: out_dir.join("protobuf"),
        };
        paths.validate()?;
        Ok(paths)
    }

    fn validate(&self) -> BuildResult<()> {
        for directory in [&self.cpp_dir, &self.sdk, &self.metamod, &self.khook_include, &self.manifests] {
            if !directory.is_dir() {
                return Err(format!("required directory is missing: {}", directory.display()).into());
            }
        }
        for file in [&self.manifest, &self.build_config, &self.plugin_metadata] {
            if !file.is_file() {
                return Err(format!("required file is missing: {}", file.display()).into());
            }
        }
        Ok(())
    }

    pub fn resolve_workspace_path(&self, relative: &str) -> BuildResult<PathBuf> {
        validate_relative(relative)?;
        Ok(self.root.join(relative))
    }
}

pub fn validate_relative(value: &str) -> BuildResult<()> {
    let path = Path::new(value);
    if value.is_empty() || path.is_absolute() || path.components().any(|part| matches!(part, std::path::Component::ParentDir)) {
        return Err(format!("configured path must be a non-empty relative path without parent traversal: {value}").into());
    }
    Ok(())
}
