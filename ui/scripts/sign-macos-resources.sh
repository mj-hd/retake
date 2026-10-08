#!/bin/bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "macOS resource signing must run on macOS" >&2
  exit 1
fi

: "${APPLE_SIGNING_IDENTITY:?APPLE_SIGNING_IDENTITY is required}"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
UI_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
RESOURCE_DIR="${1:-$UI_DIR/src-tauri/resources}"
ENTITLEMENTS="$UI_DIR/src-tauri/macos-runtime-entitlements.plist"

if [[ ! -d "$RESOURCE_DIR" ]]; then
  echo "Desktop resources are not staged: $RESOURCE_DIR" >&2
  exit 1
fi

signed=0
while IFS= read -r -d '' candidate; do
  if ! file -b "$candidate" | grep -q 'Mach-O'; then
    continue
  fi

  case "$(basename "$candidate")" in
    node|chrome-headless-shell)
      codesign \
        --force \
        --timestamp \
        --options runtime \
        --entitlements "$ENTITLEMENTS" \
        --sign "$APPLE_SIGNING_IDENTITY" \
        "$candidate"
      ;;
    *)
      codesign --force --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$candidate"
      ;;
  esac

  codesign --verify --strict --verbose=2 "$candidate"
  signed=$((signed + 1))
done < <(find "$RESOURCE_DIR" -type f -print0)

if [[ "$signed" -eq 0 ]]; then
  echo "No Mach-O resources were found under $RESOURCE_DIR" >&2
  exit 1
fi

echo "Signed and verified $signed bundled Mach-O resources"
