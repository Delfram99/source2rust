#pragma once

#include <cstdint>

namespace source2rust::logger
{

void EnableConsoleColors() noexcept;
void Write(std::uint32_t level, const char* message) noexcept;

}  // namespace source2rust::logger
