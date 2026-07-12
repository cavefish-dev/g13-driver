#!/usr/bin/env bash
# Fail if Cargo.toml's [package] version does not match version.txt.
set -euo pipefail

cargo_ver="$(grep -m1 '^version[[:space:]]*=' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"
file_ver="$(tr -d '[:space:]' < version.txt)"

if [ "$cargo_ver" != "$file_ver" ]; then
  echo "version mismatch: Cargo.toml=$cargo_ver version.txt=$file_ver" >&2
  echo "bump BOTH to the same value." >&2
  exit 1
fi
echo "version ok: $file_ver"
