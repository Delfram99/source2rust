use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

type TaskResult<T> = Result<T, Box<dyn std::error::Error>>;

const DEPLOY_MANIFEST: &str = ".source2rust-managed.json";

#[derive(Default, Deserialize, Serialize)]
struct ManagedDeployment {
    files: BTreeSet<String>,
}

#[derive(Deserialize)]
struct BuildConfig {
    #[serde(default = "default_sdk")]
    sdk: String,
    #[serde(default)]
    addon_copy_dirs: Vec<String>,
}

#[derive(Deserialize)]
struct PluginMetadata {
    name: String,
    alias: String,
}

struct BuildOptions {
    release: bool,
    sdk: Option<String>,
    addons_directory: Option<PathBuf>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> TaskResult<()> {
    let mut arguments = env::args().skip(1);
    let command = arguments.next().unwrap_or_else(|| "build".to_owned());
    let manifest_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_directory
        .parent()
        .ok_or("xtask must be located directly inside the workspace")?;

    match command.as_str() {
        "build" => {
            let options = parse_build_options(arguments)?;
            let config = load_build_config(root)?;
            let sdk = options.sdk.as_deref().unwrap_or(&config.sdk);
            validate_component(sdk, "SDK name").map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
            let out_dir = cargo_build(root, options.release, sdk)?;
            update_compilation_database(root, &out_dir)?;
            package(root, options.release, sdk, options.addons_directory.as_deref(), &config)
        }
        "fmt-cpp" => {
            if let Some(argument) = arguments.next() {
                return Err(format!("unexpected argument for fmt-cpp: {argument}").into());
            }
            format_cpp(root)
        }
        _ => Err(format!("unknown command {command}; expected build or fmt-cpp").into()),
    }
}

fn parse_build_options(mut arguments: impl Iterator<Item = String>) -> TaskResult<BuildOptions> {
    let mut release = false;
    let mut sdk = None;
    let mut addons_directory = None;
    while let Some(option) = arguments.next() {
        match option.as_str() {
            "--release" => release = true,
            "--sdk" => {
                if sdk.is_some() {
                    return Err("--sdk may only be specified once".into());
                }
                sdk = Some(arguments.next().ok_or("--sdk requires an SDK name")?);
            }
            "--addons-dir" => {
                if addons_directory.is_some() {
                    return Err("--addons-dir may only be specified once".into());
                }
                addons_directory = Some(PathBuf::from(arguments.next().ok_or("--addons-dir requires a directory path")?));
            }
            _ => return Err(format!("unknown option {option}; expected --release, --sdk, or --addons-dir").into()),
        }
    }
    Ok(BuildOptions {
        release,
        sdk,
        addons_directory,
    })
}

fn load_build_config(root: &Path) -> TaskResult<BuildConfig> {
    let path = plugin_crate(root).join("build_config.json");
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn cargo_build(root: &Path, release: bool, sdk: &str) -> TaskResult<PathBuf> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let mut command = Command::new(cargo);
    command
        .current_dir(root)
        .env("SOURCE2RUST_SDK", sdk)
        .args(["build", "--package", "source2rust", "--message-format=json-render-diagnostics"])
        .stdout(Stdio::piped());
    if release {
        command.arg("--release");
    }
    let mut child = command.spawn()?;
    let stdout = child.stdout.take().ok_or("failed to capture Cargo output")?;
    let mut out_dir = None;
    for line in BufReader::new(stdout).lines() {
        let line = line?;
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            println!("{line}");
            continue;
        };
        if message.get("reason").and_then(Value::as_str) == Some("compiler-message")
            && let Some(rendered) = message.pointer("/message/rendered").and_then(Value::as_str)
        {
            eprint!("{rendered}");
        }
        if message.get("reason").and_then(Value::as_str) == Some("build-script-executed")
            && let Some(candidate) = message.get("out_dir").and_then(Value::as_str).map(PathBuf::from)
            && candidate
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.strip_prefix("source2rust-").is_some_and(|hash| !hash.is_empty()))
        {
            out_dir = Some(candidate);
        }
    }
    let status = child.wait()?;
    if !status.success() {
        return Err(format!("cargo build failed with status {status}").into());
    }
    out_dir.ok_or_else(|| "Cargo did not report the source2rust build output directory".into())
}

fn update_compilation_database(root: &Path, out_dir: &Path) -> TaskResult<()> {
    let source = out_dir.join("compile_commands.json");
    if !source.is_file() {
        return Err(format!("compile_commands.json was not found in {}", out_dir.display()).into());
    }
    fs::copy(source, root.join("compile_commands.json"))?;
    Ok(())
}

fn package(root: &Path, release: bool, sdk: &str, addons_directory: Option<&Path>, config: &BuildConfig) -> TaskResult<()> {
    let metadata_path = plugin_crate(root).join("plugin-metadata.json");
    let metadata: PluginMetadata = serde_json::from_str(&fs::read_to_string(metadata_path)?)?;
    validate_component(&metadata.alias, "plugin alias").map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
    let profile = if release { "release" } else { "debug" };
    let target = target_directory(root).join(profile);
    let addons = addons_directory
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join("build/package").join(sdk).join("addons"));
    let bin_name = if cfg!(windows) { "win64" } else { "linuxsteamrt64" };
    let extension = if cfg!(windows) { "dll" } else { "so" };
    let source_binary = if cfg!(windows) {
        target.join(format!("{}.dll", metadata.alias))
    } else {
        target.join(format!("lib{}.so", metadata.alias))
    };
    if !source_binary.is_file() {
        return Err(format!("built plugin was not found: {}", source_binary.display()).into());
    }

    let plugin_directory = addons.join(&metadata.alias);
    let binary_directory = plugin_directory.join("bin").join(bin_name);
    let metamod_directory = addons.join("metamod");
    let mut managed_files = BTreeSet::new();
    fs::create_dir_all(&binary_directory)?;
    fs::create_dir_all(&metamod_directory)?;
    let binary_destination = binary_directory.join(format!("{}.{}", metadata.alias, extension));
    fs::copy(&source_binary, &binary_destination)?;
    record_managed_file(&addons, &binary_destination, &mut managed_files)?;

    if cfg!(windows) {
        let source_pdb = target.join(format!("{}.pdb", metadata.alias));
        if source_pdb.is_file() {
            let pdb_destination = binary_directory.join(format!("{}.pdb", metadata.alias));
            fs::copy(source_pdb, &pdb_destination)?;
            record_managed_file(&addons, &pdb_destination, &mut managed_files)?;
        }
    }

    let licenses_directory = plugin_directory.join("licenses");
    fs::create_dir_all(&licenses_directory)?;
    let protobuf_license = find_protobuf_license(root, sdk)?;
    for (source, name) in [
        (root.join("LICENSE-MIT"), "LICENSE-MIT"),
        (root.join("LICENSE-APACHE"), "LICENSE-APACHE"),
        (root.join("THIRD_PARTY_NOTICES.md"), "THIRD_PARTY_NOTICES.md"),
        (root.join("external/metamod-source/LICENSE.txt"), "metamod-source-LICENSE.txt"),
        (root.join("external/khook/LICENSE"), "khook-LICENSE.txt"),
        (protobuf_license, "protobuf-LICENSE.txt"),
    ] {
        let destination = licenses_directory.join(name);
        fs::copy(source, &destination)?;
        record_managed_file(&addons, &destination, &mut managed_files)?;
    }

    let addon_source = root.join("addons").join(sdk).join(&metadata.alias);
    for relative in &config.addon_copy_dirs {
        validate_relative_path(relative)?;
        let source = addon_source.join(relative);
        if source.is_dir() {
            copy_directory(&source, &plugin_directory.join(relative), &addons, &mut managed_files)?;
        } else if source.is_file() {
            let destination = plugin_directory.join(relative);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(source, &destination)?;
            record_managed_file(&addons, &destination, &mut managed_files)?;
        } else {
            return Err(format!("configured addon path does not exist: {}", source.display()).into());
        }
    }

    let alias = escape_vdf(&metadata.alias)?;
    let display_name = escape_vdf(&metadata.name)?;
    let vdf = format!(
        "\"Metamod Plugin\"\n{{\n    \"alias\"    \"{alias}\"\n    \"file\"     \"addons/{alias}/bin/{bin_name}/{alias}\"\n    \"name\"     \"{display_name}\"\n}}\n"
    );
    let vdf_destination = metamod_directory.join(format!("{}.vdf", metadata.alias));
    fs::write(&vdf_destination, vdf)?;
    record_managed_file(&addons, &vdf_destination, &mut managed_files)?;
    remove_stale_managed_files(&addons, &managed_files)?;
    let deployment = ManagedDeployment { files: managed_files };
    fs::write(addons.join(DEPLOY_MANIFEST), serde_json::to_vec_pretty(&deployment)?)?;
    println!("Addons: {}", addons.display());
    Ok(())
}

fn find_protobuf_license(root: &Path, sdk: &str) -> TaskResult<PathBuf> {
    let third_party = root.join("external").join(format!("hl2sdk-{sdk}")).join("thirdparty");
    let mut licenses = Vec::new();
    for entry in fs::read_dir(&third_party)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() && entry.file_name().to_string_lossy().starts_with("protobuf-") {
            let license = entry.path().join("LICENSE");
            if license.is_file() {
                licenses.push(license);
            }
        }
    }
    match licenses.as_slice() {
        [license] => Ok(license.clone()),
        [] => Err(format!("protobuf license was not found in {}", third_party.display()).into()),
        _ => Err(format!("multiple protobuf licenses were found in {}", third_party.display()).into()),
    }
}

fn copy_directory(source: &Path, destination: &Path, addons: &Path, managed_files: &mut BTreeSet<String>) -> TaskResult<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_directory(&from, &to, addons, managed_files)?;
        } else {
            fs::copy(from, &to)?;
            record_managed_file(addons, &to, managed_files)?;
        }
    }
    Ok(())
}

fn record_managed_file(addons: &Path, path: &Path, managed_files: &mut BTreeSet<String>) -> TaskResult<()> {
    let relative = path
        .strip_prefix(addons)
        .map_err(|_| format!("managed file is outside the addons directory: {}", path.display()))?;
    let value = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    validate_relative_path(&value)?;
    managed_files.insert(value);
    Ok(())
}

fn remove_stale_managed_files(addons: &Path, current_files: &BTreeSet<String>) -> TaskResult<()> {
    let manifest_path = addons.join(DEPLOY_MANIFEST);
    if !manifest_path.is_file() {
        return Ok(());
    }
    let previous: ManagedDeployment = serde_json::from_str(&fs::read_to_string(&manifest_path)?)?;
    for relative in previous.files.difference(current_files) {
        validate_relative_path(relative)?;
        let stale = addons.join(relative);
        if stale.is_file() || stale.is_symlink() {
            fs::remove_file(&stale)?;
            remove_empty_parents(stale.parent(), addons);
        }
    }
    Ok(())
}

fn remove_empty_parents(mut directory: Option<&Path>, boundary: &Path) {
    while let Some(path) = directory {
        if path == boundary || !path.starts_with(boundary) || fs::remove_dir(path).is_err() {
            break;
        }
        directory = path.parent();
    }
}

fn format_cpp(root: &Path) -> TaskResult<()> {
    const FORMATTERS: [&str; 7] = [
        "clang-format",
        "clang-format-19",
        "clang-format-18",
        "clang-format-17",
        "clang-format-16",
        "clang-format-15",
        "clang-format-14",
    ];
    let mut files = Vec::new();
    collect_cpp_files(&plugin_crate(root).join("cpp"), &mut files)?;
    for formatter in FORMATTERS {
        match Command::new(formatter).args(["-i", "--style=file"]).args(&files).status() {
            Ok(status) if status.success() => return Ok(()),
            Ok(status) => return Err(format!("{formatter} failed with status {status}").into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err("clang-format was not found".into())
}

fn collect_cpp_files(directory: &Path, files: &mut Vec<PathBuf>) -> TaskResult<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_cpp_files(&path, files)?;
        } else if matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("cpp" | "hpp" | "h" | "inl")
        ) {
            files.push(path);
        }
    }
    Ok(())
}

fn plugin_crate(root: &Path) -> PathBuf {
    root.join("crates/source2rust_mm")
}

fn target_directory(root: &Path) -> PathBuf {
    match env::var_os("CARGO_TARGET_DIR") {
        Some(directory) => {
            let directory = PathBuf::from(directory);
            if directory.is_absolute() { directory } else { root.join(directory) }
        }
        None => root.join("target"),
    }
}

fn validate_relative_path(value: &str) -> TaskResult<()> {
    let path = Path::new(value);
    if value.is_empty() || path.is_absolute() || path.components().any(|part| matches!(part, std::path::Component::ParentDir)) {
        return Err(format!("addon path must be relative and cannot contain parent traversal: {value}").into());
    }
    Ok(())
}

fn escape_vdf(value: &str) -> TaskResult<String> {
    if value.contains(['\n', '\r', '\0']) {
        return Err("VDF values cannot contain control line characters".into());
    }
    Ok(value.replace('\\', "\\\\").replace('"', "\\\""))
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
