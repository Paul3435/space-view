#!/usr/bin/env bash
# Cross-compiles a portable Windows x86_64 disktree.exe from Linux/macOS.
#
# Requirements (one-time):
#   rustup target add x86_64-pc-windows-msvc
#   cargo install cargo-xwin            # downloads the MSVC CRT + Windows SDK on first use
#   llvm-rc on PATH (Ubuntu: sudo apt install llvm-18, it lives in /usr/lib/llvm-18/bin)
#
# The C runtime is linked statically (.cargo/config.toml), so the exe has no
# dependency on the Visual C++ Redistributable.
set -euo pipefail
cd "$(dirname "$0")/.."

for d in /usr/lib/llvm-*/bin; do
  [ -x "$d/llvm-rc" ] && export PATH="$PATH:$d"
done
command -v llvm-rc >/dev/null || { echo "llvm-rc not found (install llvm)"; exit 1; }
command -v cargo-xwin >/dev/null || { echo "cargo-xwin not found: cargo install cargo-xwin"; exit 1; }

cargo xwin build --release --target x86_64-pc-windows-msvc "$@"

exe=target/x86_64-pc-windows-msvc/release/disktree.exe
mkdir -p dist
cp "$exe" dist/disktree.exe
echo "Built dist/disktree.exe ($(du -h dist/disktree.exe | cut -f1))"
