use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use source2rust_abi::layout::ABI_LAYOUT_REGISTRY;

use super::BuildResult;
use super::config::PluginMetadata;

pub fn generate_version_header(output: &Path, metadata: &PluginMetadata, version: &str) -> BuildResult<()> {
    let name = escape_cpp(&metadata.name)?;
    let author = escape_cpp(&metadata.author)?;
    let description = escape_cpp(&metadata.description)?;
    let url = escape_cpp(&metadata.url)?;
    let license = escape_cpp(&metadata.license)?;
    let log_tag = escape_cpp(metadata.log_tag())?;
    let version = escape_cpp(version)?;
    let contents = format!(
        "#pragma once\n\n#define PLUGIN_NAME \"{}\"\n#define PLUGIN_AUTHOR \"{}\"\n#define PLUGIN_DESCRIPTION \"{}\"\n#define PLUGIN_URL \"{}\"\n#define PLUGIN_LICENSE \"{}\"\n#define PLUGIN_LOG_TAG \"{}\"\n#define PLUGIN_VERSION \"{}\"\n",
        name, author, description, url, license, log_tag, version,
    );
    write_if_changed(output, contents.as_bytes())
}

pub fn install_generated_header(source: &Path, output: &Path) -> BuildResult<()> {
    let contents = fs::read(source).map_err(|error| format!("failed to read generated header {}: {error}", source.display()))?;
    write_if_changed(output, &contents)
}

pub fn generate_abi_layout(output: &Path) -> BuildResult<()> {
    let mut contents = String::from(concat!(
        "/**\n",
        " * Rust/C++ ABI layout checks.\n",
        " *\n",
        " * Generated automatically from source2rust_abi.\n",
        " */\n\n",
        "#ifndef S2R_RUST_ABI_LAYOUT_INL\n",
        "#define S2R_RUST_ABI_LAYOUT_INL\n\n",
        "#include \"rust_abi.hpp\"\n",
        "#include <cstddef>\n\n",
    ));

    for entry in ABI_LAYOUT_REGISTRY {
        writeln!(
            contents,
            "static_assert(sizeof({0}) == {1}u, \"sizeof({0}) does not match Rust\");",
            entry.name, entry.size
        )?;
        writeln!(
            contents,
            "static_assert(alignof({0}) == {1}u, \"alignof({0}) does not match Rust\");",
            entry.name, entry.align
        )?;
        for field in entry.fields {
            writeln!(
                contents,
                "static_assert(offsetof({0}, {1}) == {2}u, \"offsetof({0}, {1}) does not match Rust\");",
                entry.name, field.name, field.offset
            )?;
        }
        contents.push('\n');
    }

    contents.push_str("#endif  // S2R_RUST_ABI_LAYOUT_INL\n");
    write_if_changed(output, contents.as_bytes())
}

pub fn write_if_changed(path: &Path, contents: &[u8]) -> BuildResult<()> {
    if fs::read(path).is_ok_and(|existing| existing == contents) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    fs::write(path, contents).map_err(|error| format!("failed to write {}: {error}", path.display()).into())
}

fn escape_cpp(value: &str) -> BuildResult<String> {
    if value.chars().any(char::is_control) {
        return Err("plugin metadata cannot contain control characters".into());
    }
    Ok(value.replace('\\', "\\\\").replace('"', "\\\""))
}
