#include "plugin.hpp"

#include "core/logger.hpp"
#include "rust_abi_layout.inl"
#include "source2rust_ffi.hpp"

#include <cstdio>

namespace source2rust
{

namespace
{

void SetLoadError(char* error, size_t max_length, const char* message)
{
    if (error != nullptr && max_length > 0)
    {
        std::snprintf(error, max_length, "%s", message);
    }
}

}  // namespace

Source2RustPlugin gPlugin;

Source2RustPlugin::Source2RustPlugin()
{
    InitializeHostApi();
}

const void* Source2RustPlugin::GetInterface(const char*, int) noexcept
{
    return nullptr;
}

bool Source2RustPlugin::Load(PluginId id, ISmmAPI* ismm, char* error, size_t max_length, bool /*late*/)
{
    PLUGIN_SAVEVARS();
    g_SMAPI->AddListener(this, this);
    logger::EnableConsoleColors();

    const std::uint64_t abi_fingerprint = (static_cast<std::uint64_t>(ABI_MAGIC) << 32u) | API_VERSION;
    if (s2r_abi_fingerprint() != abi_fingerprint)
    {
        SetLoadError(error, max_length, "Source2Rust ABI mismatch");
        return false;
    }

    if (!s2r_core_start(&m_hostApi))
    {
        s2r_core_stop();
        SetLoadError(error, max_length, "Source2Rust Rust core failed to start");
        return false;
    }

    return true;
}

bool Source2RustPlugin::Unload(char* /*error*/, size_t /*max_length*/)
{
    s2r_core_stop();

    return true;
}

}  // namespace source2rust

PLUGIN_EXPOSE(Source2RustPlugin, source2rust::gPlugin);
