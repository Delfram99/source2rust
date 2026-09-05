#include "../plugin.hpp"

#include "logger.hpp"

namespace source2rust
{

void Source2RustPlugin::InitializeHostApi()
{
    m_hostApi.version = static_cast<int>(API_VERSION);
    m_hostApi.get_interface = &GetInterface;
    m_hostApi.log = &logger::Write;
}

}  // namespace source2rust
