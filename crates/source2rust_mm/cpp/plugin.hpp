#pragma once

#include <ISmmPlugin.h>

#include "rust_abi.hpp"
#include "version_gen.hpp"

#if __cplusplus < 202002L
#error Source2Rust requires C++20 or newer
#endif

PLUGIN_GLOBALVARS();

namespace source2rust
{

class Source2RustPlugin final : public ISmmPlugin, public IMetamodListener
{
public:
    Source2RustPlugin();

    bool Load(PluginId id, ISmmAPI* ismm, char* error, size_t max_length, bool late) override;
    bool Unload(char* error, size_t max_length) override;

    const char* GetAuthor() override
    {
        return PLUGIN_AUTHOR;
    }

    const char* GetName() override
    {
        return PLUGIN_NAME;
    }

    const char* GetDescription() override
    {
        return PLUGIN_DESCRIPTION;
    }

    const char* GetURL() override
    {
        return PLUGIN_URL;
    }

    const char* GetLicense() override
    {
        return PLUGIN_LICENSE;
    }

    const char* GetVersion() override
    {
        return PLUGIN_VERSION;
    }

    const char* GetDate() override
    {
        return __DATE__;
    }

    const char* GetLogTag() override
    {
        return PLUGIN_LOG_TAG;
    }

private:
    void InitializeHostApi();
    static const void* GetInterface(const char* name, int version) noexcept;

    S2RHostApi m_hostApi{};
};

extern Source2RustPlugin gPlugin;

}  // namespace source2rust
