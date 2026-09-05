mod config;
mod headers;
mod native;
mod paths;

use std::env;
use std::error::Error;
use std::path::PathBuf;

use config::{BuildConfig, PluginMetadata, SdkManifest, load_json};
use paths::ProjectPaths;

pub type BuildResult<T> = Result<T, Box<dyn Error>>;

pub fn run() -> BuildResult<()> {
    let crate_root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is not set")?);
    let config: BuildConfig = load_json(&crate_root.join("build_config.json"))?;
    config.validate().map_err(|error| -> Box<dyn Error> { error.into() })?;
    let sdk_name = env::var("SOURCE2RUST_SDK").unwrap_or_else(|_| config.sdk.clone());
    let paths = ProjectPaths::discover(&sdk_name)?;
    let metadata: PluginMetadata = load_json(&paths.plugin_metadata)?;
    metadata.validate().map_err(|error| -> Box<dyn Error> { error.into() })?;
    let manifest: SdkManifest = load_json(&paths.manifest)?;

    if manifest.name != sdk_name {
        return Err(format!("SDK manifest name {} does not match requested SDK {sdk_name}", manifest.name).into());
    }
    if manifest.extension.is_empty() {
        return Err(format!("SDK manifest {} has an empty extension", paths.manifest.display()).into());
    }

    let package_name = env::var("CARGO_PKG_NAME")?;
    let package_version = env::var("CARGO_PKG_VERSION")?;
    if package_name != metadata.alias {
        return Err(format!("Cargo package name {package_name} does not match plugin alias {}", metadata.alias).into());
    }

    headers::generate_version_header(&paths.generated.join("version_gen.hpp"), &metadata, &package_version)?;
    let abi_header = PathBuf::from(env::var_os("DEP_SOURCE2RUST_CORE_ABI_HEADER").ok_or("DEP_SOURCE2RUST_CORE_ABI_HEADER is not set")?);
    headers::install_generated_header(&abi_header, &paths.generated.join("rust_abi.hpp"))?;
    headers::generate_abi_layout(&paths.generated.join("rust_abi_layout.inl"))?;
    let ffi_header = PathBuf::from(env::var_os("DEP_SOURCE2RUST_CORE_FFI_HEADER").ok_or("DEP_SOURCE2RUST_CORE_FFI_HEADER is not set")?);
    headers::install_generated_header(&ffi_header, &paths.generated.join("source2rust_ffi.hpp"))?;
    let engine_defines = manifest.engine_defines(&paths.manifests)?;
    native::build(&paths, &config, &manifest, &engine_defines)?;
    Ok(())
}
