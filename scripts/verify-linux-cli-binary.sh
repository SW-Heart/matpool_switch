#!/usr/bin/env bash
set -euo pipefail

binary_path=${1:?Usage: verify-linux-cli-binary.sh <path-to-matpool>}

if [[ ! -x "$binary_path" ]]; then
  echo "Linux CLI binary is missing or not executable: $binary_path" >&2
  exit 1
fi

# Release artifacts use a fully static musl binary. This is intentional: it
# prevents the host distribution's GLIBC and OpenSSL ABI from affecting startup.
if readelf -d "$binary_path" | grep -q '(NEEDED)'; then
  echo "Linux CLI must not have dynamic library dependencies:" >&2
  readelf -d "$binary_path" >&2
  exit 1
fi

file "$binary_path"
echo "Linux CLI compatibility check passed: static, with no host GLIBC/OpenSSL dependency."
