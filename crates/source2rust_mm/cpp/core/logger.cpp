#include "logger.hpp"

#include "../plugin.hpp"
#include "rust_abi.hpp"
#include "version_gen.hpp"

#ifdef _WIN32
#include <windows.h>
#endif

namespace source2rust::logger
{

namespace
{

constexpr const char* kAnsiReset = "\x1b[0m";
constexpr const char* kAnsiBoldOrange = "\x1b[1;38;2;255;165;0m";
constexpr const char* kAnsiWhite = "\x1b[38;2;238;238;238m";
constexpr const char* kAnsiBoldYellow = "\x1b[1;38;2;181;137;0m";
constexpr const char* kAnsiBoldRed = "\x1b[1;38;2;220;50;47m";
constexpr const char* kAnsiGreen = "\x1b[38;2;0;190;0m";

struct Style
{
    const char* name;
    const char* color;
};

Style GetStyle(std::uint32_t level) noexcept
{
    switch (level)
    {
        case LOG_WARN:
            return {"WARN", kAnsiBoldYellow};
        case LOG_ERROR:
            return {"ERROR", kAnsiBoldRed};
        default:
            return {"INFO", kAnsiWhite};
    }
}

}  // namespace

void EnableConsoleColors() noexcept
{
#ifdef _WIN32
    const HANDLE output = GetStdHandle(STD_OUTPUT_HANDLE);
    if (output == nullptr || output == INVALID_HANDLE_VALUE)
    {
        return;
    }

    DWORD mode = 0;
    if (GetConsoleMode(output, &mode))
    {
        SetConsoleMode(output, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
    }
#endif
}

void Write(std::uint32_t level, const char* message) noexcept
{
    const Style style = GetStyle(level);
    META_CONPRINTF("%s[%s]%s%s[%s]%s %s%s%s\n", kAnsiBoldOrange, PLUGIN_NAME, kAnsiReset, style.color, style.name, kAnsiReset, kAnsiGreen,
                   message != nullptr ? message : "<null>", kAnsiReset);
}

}  // namespace source2rust::logger
