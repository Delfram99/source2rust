use std::env;
use std::error::Error;
use std::io;
use std::path::PathBuf;

type BuildResult<T> = Result<T, Box<dyn Error>>;

fn main() {
    if let Err(error) = generate_abi_header() {
        eprintln!("failed to generate the Source2Rust ABI header: {error}");
        std::process::exit(1);
    }
}

fn generate_abi_header() -> BuildResult<()> {
    let crate_root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is not set")?);
    let output_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let config_path = crate_root.join("cbindgen.toml");
    let output_path = output_dir.join("rust_abi.hpp");

    let config = cbindgen::Config::from_file(&config_path)
        .map_err(|error| io::Error::other(format!("failed to read {}: {error}", config_path.display())))?;
    let bindings = cbindgen::Builder::new()
        .with_crate(&crate_root)
        .with_config(config)
        .generate()
        .map_err(|error| io::Error::other(format!("cbindgen failed: {error}")))?;
    bindings.write_to_file(&output_path);

    println!("cargo::metadata=header={}", output_path.display());
    println!("cargo::rerun-if-changed={}", config_path.display());
    println!("cargo::rerun-if-changed={}", crate_root.join("src").display());
    Ok(())
}
