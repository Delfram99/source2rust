# Source2Rust

An experimental project for playing around with Rust in Source 2 games.

If you want to write full-fledged plugins in Rust, use [Plugify](https://plugify.net/languages/rust/first-plugin).

## Status

This project is still in an early experimental stage. There are no useful server features, so there's nothing to use.

## Requirements

Only works with Metamod:Source built from the `k/sourcehook_alternative` branch.
Supports CS2.

## Building

- Rust 1.98.1 via rustup.
- Windows x64: MSVC Build Tools with C++20 support and Windows SDK.
- Linux x64: GCC or Clang with C++20 support and libstdc++ development files.

Build on Windows or Linux:

```sh
git clone --recurse-submodules https://github.com/Delfram99/source2rust.git
cd source2rust
cargo xtask build --release
```

For a debug build, run `cargo xtask build` without `--release`.
