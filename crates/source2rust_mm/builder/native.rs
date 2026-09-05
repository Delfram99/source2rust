use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::BuildResult;
use super::config::{ArchitectureSdk, BuildConfig, PlatformSdk, SdkManifest};
use super::headers::write_if_changed;
use super::paths::{ProjectPaths, validate_relative};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Target {
    WindowsX64,
    LinuxX64,
}

impl Target {
    fn from_environment() -> BuildResult<Self> {
        match std::env::var("TARGET")?.as_str() {
            "x86_64-pc-windows-msvc" => Ok(Self::WindowsX64),
            "x86_64-unknown-linux-gnu" => Ok(Self::LinuxX64),
            target => Err(format!(
                "unsupported target {target}; supported targets are x86_64-pc-windows-msvc and x86_64-unknown-linux-gnu"
            )
            .into()),
        }
    }

    fn platform(self, sdk: &SdkManifest) -> BuildResult<&PlatformSdk> {
        match self {
            Self::WindowsX64 => sdk.windows.as_ref(),
            Self::LinuxX64 => sdk.linux.as_ref(),
        }
        .ok_or_else(|| format!("SDK {} does not define the requested target platform", sdk.name).into())
    }

    fn architecture<'a>(self, platform: &'a PlatformSdk, sdk_name: &str) -> BuildResult<&'a ArchitectureSdk> {
        platform
            .x86_64
            .as_ref()
            .ok_or_else(|| format!("SDK {sdk_name} does not define x86_64 for the requested platform").into())
    }
}

pub fn build(paths: &ProjectPaths, config: &BuildConfig, sdk: &SdkManifest, engine_defines: &[String]) -> BuildResult<()> {
    let target = Target::from_environment()?;
    let platform = target.platform(sdk)?;
    let architecture = target.architecture(platform, &sdk.name)?;
    let project_sources = resolve_sources(&paths.crate_root, &config.project_sources, "project source")?;
    let sdk_sources = resolve_sources(&paths.sdk, &config.sdk_sources, "SDK source")?;
    let proto_inputs = resolve_proto_sources(paths, sdk, &config.proto_files, false)?;
    let optional_proto_inputs = resolve_proto_sources(paths, sdk, &config.proto_optional_files, true)?;
    let all_proto_inputs = proto_inputs.into_iter().chain(optional_proto_inputs).collect::<Vec<_>>();
    let protobuf_sources = generate_protobuf(paths, sdk, platform, &all_proto_inputs)?;

    let mut dependency_sources = sdk_sources;
    dependency_sources.extend(protobuf_sources);
    let includes = collect_includes(paths, config, sdk)?;
    let defines = collect_defines(config, platform, engine_defines, target);

    compile_cpp(paths, config, target, &project_sources, &dependency_sources, &includes, &defines)?;
    emit_linker_configuration(paths, config, platform, architecture, target)?;
    emit_rerun_directives(paths, &project_sources, &dependency_sources, &all_proto_inputs);
    Ok(())
}

#[derive(Debug)]
struct ProtoInput {
    path: PathBuf,
    relative: PathBuf,
}

fn resolve_proto_sources(paths: &ProjectPaths, sdk: &SdkManifest, sources: &[String], optional: bool) -> BuildResult<Vec<ProtoInput>> {
    let roots = sdk.include_paths.iter().map(|include| paths.sdk.join(include)).collect::<Vec<_>>();
    let mut resolved = Vec::new();
    for source in sources {
        validate_relative(source)?;
        let relative = PathBuf::from(source);
        if let Some(path) = roots.iter().map(|root| root.join(&relative)).find(|path| path.is_file()) {
            resolved.push(ProtoInput { path, relative });
        } else if !optional {
            return Err(format!("protobuf source does not exist in an SDK protobuf root: {source}").into());
        }
    }
    Ok(resolved)
}

fn generate_protobuf(paths: &ProjectPaths, sdk: &SdkManifest, platform: &PlatformSdk, inputs: &[ProtoInput]) -> BuildResult<Vec<PathBuf>> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    let protoc_relative = platform
        .protoc_path
        .as_deref()
        .ok_or("the selected SDK platform does not define protoc_path")?;
    validate_relative(protoc_relative)?;
    let protoc = paths.sdk.join(protoc_relative);
    if !protoc.is_file() {
        return Err(format!("protoc does not exist: {}", protoc.display()).into());
    }

    fs::create_dir_all(&paths.protobuf)?;
    let sources = protobuf_outputs(inputs, &paths.protobuf, "pb.cc")?;
    let headers = protobuf_outputs(inputs, &paths.protobuf, "pb.h")?;

    sdk.include_paths
        .iter()
        .map(|include| paths.sdk.join(include))
        .find(|include| include.join("google/protobuf/descriptor.proto").is_file())
        .ok_or_else(|| format!("SDK {} does not expose a protobuf include directory", sdk.name))?;
    let mut command = Command::new(&protoc);
    for include in sdk.include_paths.iter().map(|include| paths.sdk.join(include)) {
        command.arg(format!("--proto_path={}", include.display()));
    }
    let status = command
        .arg(format!("--cpp_out={}", paths.protobuf.display()))
        .args(inputs.iter().map(|input| &input.path))
        .status()
        .map_err(|error| format!("failed to run {}: {error}", protoc.display()))?;
    if !status.success() {
        return Err(format!("protoc failed with status {status}").into());
    }
    for output in sources.iter().chain(&headers) {
        if !output.is_file() {
            return Err(format!("protoc did not generate expected output: {}", output.display()).into());
        }
    }
    Ok(sources)
}

fn protobuf_outputs(inputs: &[ProtoInput], output: &Path, extension: &str) -> BuildResult<Vec<PathBuf>> {
    let mut unique = BTreeSet::new();
    let mut outputs = Vec::new();
    for input in inputs {
        let path = output.join(&input.relative).with_extension(extension);
        if !unique.insert(path.clone()) {
            return Err(format!("multiple protobuf inputs produce the same output: {}", path.display()).into());
        }
        outputs.push(path);
    }
    Ok(outputs)
}

fn resolve_sources(root: &Path, sources: &[String], kind: &str) -> BuildResult<Vec<PathBuf>> {
    sources
        .iter()
        .map(|source| {
            validate_relative(source)?;
            let path = root.join(source);
            if !path.is_file() {
                return Err(format!("{kind} does not exist: {}", path.display()).into());
            }
            Ok(path)
        })
        .collect()
}

struct IncludePaths {
    project: Vec<PathBuf>,
    external: Vec<PathBuf>,
}

impl IncludePaths {
    fn all(&self) -> impl Iterator<Item = &PathBuf> {
        self.project.iter().chain(&self.external)
    }
}

fn collect_includes(paths: &ProjectPaths, config: &BuildConfig, sdk: &SdkManifest) -> BuildResult<IncludePaths> {
    let project = vec![paths.cpp_dir.clone(), paths.generated.clone(), paths.protobuf.clone()];
    let mut external = vec![paths.metamod.join("core"), paths.khook_include.clone()];
    external.extend(sdk.include_paths.iter().map(|include| paths.sdk.join(include)));
    for include in &config.additional_include_paths {
        let path = paths.resolve_workspace_path(include)?;
        if !path.is_dir() {
            return Err(format!("additional include directory does not exist: {}", path.display()).into());
        }
        external.push(path);
    }
    Ok(IncludePaths { project, external })
}

fn collect_defines(
    config: &BuildConfig,
    platform: &PlatformSdk,
    engine_defines: &[String],
    target: Target,
) -> Vec<(String, Option<String>)> {
    let mut raw = vec![
        "_CRT_SECURE_NO_DEPRECATE".to_owned(),
        "_CRT_SECURE_NO_WARNINGS".to_owned(),
        "_CRT_NONSTDC_NO_DEPRECATE".to_owned(),
        "GAME_DLL".to_owned(),
        "RAD_TELEMETRY_DISABLED".to_owned(),
        "X64BITS".to_owned(),
        "PLATFORM_64BITS".to_owned(),
    ];
    match target {
        Target::WindowsX64 => raw.extend(["WIN32", "_WINDOWS", "WIN64", "COMPILER_MSVC", "COMPILER_MSVC64"].map(str::to_owned)),
        Target::LinuxX64 => raw.extend(
            [
                "POSIX",
                "LINUX",
                "_LINUX",
                "GNUC",
                "GNUCLIKE",
                "COMPILER_GCC",
                "stricmp=strcasecmp",
                "_stricmp=strcasecmp",
                "_snprintf=snprintf",
                "_vsnprintf=vsnprintf",
                "HAVE_STDINT_H",
                "_FILE_OFFSET_BITS=64",
            ]
            .map(str::to_owned),
        ),
    }
    raw.extend(engine_defines.iter().cloned());
    raw.extend(platform.defines.iter().cloned());
    raw.extend(config.defines.iter().cloned());
    raw.into_iter().map(split_define).collect()
}

fn split_define(value: String) -> (String, Option<String>) {
    match value.split_once('=') {
        Some((name, value)) => (name.to_owned(), Some(value.to_owned())),
        None => (value, None),
    }
}

fn compile_cpp(
    paths: &ProjectPaths,
    config: &BuildConfig,
    target: Target,
    project_sources: &[PathBuf],
    dependency_sources: &[PathBuf],
    includes: &IncludePaths,
    defines: &[(String, Option<String>)],
) -> BuildResult<()> {
    let mut project = base_build(config, target, defines);
    project.warnings(true).extra_warnings(true);
    for include in &includes.project {
        project.include(include);
    }
    for include in &includes.external {
        match target {
            Target::WindowsX64 => {
                project.flag(format!("/external:I{}", include.display()));
            }
            Target::LinuxX64 => {
                project.flag("-isystem").flag(include.as_os_str());
            }
        }
    }
    if target == Target::WindowsX64 {
        project.flag_if_supported("/external:W0");
    }
    project.files(project_sources).cargo_metadata(false);
    let compiler = project.get_compiler().path().to_path_buf();
    project.try_compile("source2rust_native")?;

    let mut dependencies = base_build(config, target, defines);
    dependencies.warnings(false);
    for include in includes.all() {
        dependencies.include(include);
    }
    dependencies.files(dependency_sources).cargo_metadata(false);
    dependencies.try_compile("source2rust_dependencies")?;

    let all_sources = project_sources.iter().chain(dependency_sources).cloned().collect::<Vec<_>>();
    let all_includes = includes.all().cloned().collect::<Vec<_>>();
    write_compilation_database(CompilationDatabaseInput {
        path: &paths.out_dir.join("compile_commands.json"),
        root: &paths.root,
        compiler: &compiler,
        sources: &all_sources,
        includes: &all_includes,
        defines,
        standard: &config.cxx_standard,
        target,
    })?;

    println!("cargo::rustc-link-search=native={}", paths.out_dir.display());
    match target {
        Target::WindowsX64 => {
            println!("cargo::rustc-link-arg-cdylib=/WHOLEARCHIVE:source2rust_native.lib");
            println!("cargo::rustc-link-arg-cdylib=/WHOLEARCHIVE:source2rust_dependencies.lib");
        }
        Target::LinuxX64 => {
            println!("cargo::rustc-link-arg-cdylib=-Wl,--whole-archive");
            println!("cargo::rustc-link-arg-cdylib=-lsource2rust_native");
            println!("cargo::rustc-link-arg-cdylib=-lsource2rust_dependencies");
            println!("cargo::rustc-link-arg-cdylib=-Wl,--no-whole-archive");
        }
    }
    Ok(())
}

fn base_build(config: &BuildConfig, target: Target, defines: &[(String, Option<String>)]) -> cc::Build {
    let mut build = cc::Build::new();
    build.cpp(true).std(&config.cxx_standard);
    match target {
        Target::WindowsX64 => {
            build
                .static_crt(true)
                .flag_if_supported("/utf-8")
                .flag_if_supported("/Zc:__cplusplus")
                .flag_if_supported("/Oy-");
        }
        Target::LinuxX64 => {
            build
                .flag_if_supported("-fPIC")
                .flag_if_supported("-fvisibility=hidden")
                .flag_if_supported("-fvisibility-inlines-hidden")
                .flag_if_supported("-fno-strict-aliasing")
                .flag_if_supported("-fno-threadsafe-statics")
                .flag_if_supported("-msse")
                .flag_if_supported("-mfpmath=sse");
        }
    }
    for (name, value) in defines {
        build.define(name, value.as_deref());
    }
    build
}

fn emit_linker_configuration(
    paths: &ProjectPaths,
    config: &BuildConfig,
    platform: &PlatformSdk,
    architecture: &ArchitectureSdk,
    target: Target,
) -> BuildResult<()> {
    for library in architecture.libs.iter().chain(&architecture.postlink_libs) {
        emit_sdk_library(paths, library, target, true)?;
    }
    for library in &architecture.dynamic_libs {
        emit_sdk_library(paths, library, target, false)?;
    }

    match target {
        Target::WindowsX64 => {
            for library in [
                "legacy_stdio_definitions",
                "kernel32",
                "user32",
                "gdi32",
                "winspool",
                "comdlg32",
                "advapi32",
                "shell32",
                "ole32",
                "oleaut32",
                "uuid",
                "odbc32",
                "odbccp32",
                "ws2_32",
                "userenv",
                "bcrypt",
                "ntdll",
                "dbghelp",
                "synchronization",
            ] {
                println!("cargo::rustc-link-lib=dylib={library}");
            }
            println!("cargo::rustc-link-arg-cdylib=/SUBSYSTEM:WINDOWS");
            println!("cargo::rustc-link-arg-cdylib=/IGNORE:4099");
            for export in &config.windows_exports {
                validate_symbol(export)?;
                println!("cargo::rustc-link-arg-cdylib=/EXPORT:{export}");
            }
        }
        Target::LinuxX64 => {
            for library in ["dl", "pthread", "m"] {
                println!("cargo::rustc-link-lib=dylib={library}");
            }
            if platform.uses_system_cxxlib {
                println!("cargo::rustc-link-lib=dylib=stdc++");
            } else {
                println!("cargo::rustc-link-arg-cdylib=-static-libstdc++");
            }
            if !config.linux_exports.is_empty() {
                let mut script = String::from("{ global:");
                for export in &config.linux_exports {
                    validate_symbol(export)?;
                    script.push(' ');
                    script.push_str(export);
                    script.push(';');
                }
                script.push_str(" };\n");
                let path = paths.out_dir.join("cpp_exports.ver");
                write_if_changed(&path, script.as_bytes())?;
                println!("cargo::rustc-link-arg-cdylib=-Wl,--version-script={}", path.display());
            }
        }
    }
    Ok(())
}

fn emit_sdk_library(paths: &ProjectPaths, relative: &str, target: Target, static_library: bool) -> BuildResult<()> {
    validate_relative(relative)?;
    let path = paths.sdk.join(relative);
    if !path.is_file() {
        return Err(format!("SDK library does not exist: {}", path.display()).into());
    }
    let parent = path
        .parent()
        .ok_or_else(|| format!("SDK library has no parent: {}", path.display()))?;
    let file = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("SDK library has an invalid file name: {}", path.display()))?;
    println!("cargo::rustc-link-search=native={}", parent.display());
    match (target, static_library) {
        (Target::WindowsX64, true) => {
            let name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .ok_or_else(|| format!("invalid library name: {file}"))?;
            println!("cargo::rustc-link-lib=static={name}");
        }
        (Target::WindowsX64, false) => {
            let name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .ok_or_else(|| format!("invalid library name: {file}"))?;
            println!("cargo::rustc-link-lib=dylib={name}");
        }
        (Target::LinuxX64, true) => println!("cargo::rustc-link-lib=static:+verbatim={file}"),
        (Target::LinuxX64, false) => {
            let name = file.strip_prefix("lib").unwrap_or(file).strip_suffix(".so").unwrap_or(file);
            println!("cargo::rustc-link-lib=dylib={name}");
        }
    }
    Ok(())
}

fn validate_symbol(symbol: &str) -> BuildResult<()> {
    if symbol.is_empty()
        || !symbol
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'?' | b'@' | b'$'))
    {
        return Err(format!("invalid linker export symbol: {symbol}").into());
    }
    Ok(())
}

fn emit_rerun_directives(paths: &ProjectPaths, project_sources: &[PathBuf], dependency_sources: &[PathBuf], proto_inputs: &[ProtoInput]) {
    for source in project_sources.iter().chain(dependency_sources) {
        if !source.starts_with(&paths.out_dir) {
            println!("cargo::rerun-if-changed={}", source.display());
        }
    }
    for header in walk_headers(&paths.cpp_dir) {
        println!("cargo::rerun-if-changed={}", header.display());
    }
    for file in [&paths.plugin_metadata, &paths.build_config, &paths.manifest] {
        println!("cargo::rerun-if-changed={}", file.display());
    }
    for proto in proto_inputs {
        println!("cargo::rerun-if-changed={}", proto.path.display());
    }
    println!("cargo::rerun-if-changed={}", paths.sdk.display());
    println!("cargo::rerun-if-changed={}", paths.metamod.join("core").display());
    println!("cargo::rerun-if-changed={}", paths.khook_include.display());
    println!("cargo::rerun-if-changed={}", paths.manifests.display());
    println!("cargo::rerun-if-env-changed=SOURCE2RUST_SDK");
    println!("cargo::rerun-if-env-changed=CC");
    println!("cargo::rerun-if-env-changed=CXX");
}

fn walk_headers(directory: &Path) -> Vec<PathBuf> {
    let mut headers = Vec::new();
    if let Ok(entries) = fs::read_dir(directory) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                headers.extend(walk_headers(&path));
            } else if matches!(path.extension().and_then(|extension| extension.to_str()), Some("h" | "hpp" | "inl")) {
                headers.push(path);
            }
        }
    }
    headers
}

#[derive(Serialize)]
struct CompilationEntry {
    directory: String,
    file: String,
    arguments: Vec<String>,
}

struct CompilationDatabaseInput<'a> {
    path: &'a Path,
    root: &'a Path,
    compiler: &'a Path,
    sources: &'a [PathBuf],
    includes: &'a [PathBuf],
    defines: &'a [(String, Option<String>)],
    standard: &'a str,
    target: Target,
}

fn write_compilation_database(input: CompilationDatabaseInput<'_>) -> BuildResult<()> {
    let mut common = vec![input.compiler.display().to_string()];
    match input.target {
        Target::WindowsX64 => common.extend([
            format!("/std:{}", input.standard),
            "/utf-8".to_owned(),
            "/Zc:__cplusplus".to_owned(),
            "/TP".to_owned(),
            "/W4".to_owned(),
        ]),
        Target::LinuxX64 => common.extend([
            format!("-std={}", input.standard),
            "-x".to_owned(),
            "c++".to_owned(),
            "-Wall".to_owned(),
            "-Wextra".to_owned(),
        ]),
    }
    for include in input.includes {
        match input.target {
            Target::WindowsX64 => common.push(format!("/I{}", include.display())),
            Target::LinuxX64 => common.push(format!("-I{}", include.display())),
        }
    }
    for (name, value) in input.defines {
        let prefix = if input.target == Target::WindowsX64 { "/D" } else { "-D" };
        common.push(match value {
            Some(value) => format!("{prefix}{name}={value}"),
            None => format!("{prefix}{name}"),
        });
    }
    let entries = input
        .sources
        .iter()
        .map(|source| {
            let mut arguments = common.clone();
            arguments.extend([
                if input.target == Target::WindowsX64 { "/c" } else { "-c" }.to_owned(),
                source.display().to_string(),
            ]);
            CompilationEntry {
                directory: input.root.display().to_string(),
                file: source.display().to_string(),
                arguments,
            }
        })
        .collect::<Vec<_>>();
    write_if_changed(input.path, serde_json::to_string_pretty(&entries)?.as_bytes())
}
