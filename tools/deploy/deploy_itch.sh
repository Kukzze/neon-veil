#!/usr/bin/env bash
# Builds, packages and pushes Neon Veil to itch.io with butler (Linux).
#
# Usage: tools/deploy/deploy_itch.sh <version> [platform] [--skip-build] [--dry-run]
# Run from the repository root. Requires butler on PATH (except --dry-run).
set -euo pipefail

VERSION_TAG=""
TARGET_PLATFORM="linux-x64"
PROJECT="cutting-onion-studios/neon-veil"
SKIP_BUILD=0
DRY_RUN=0

for arg in "$@"; do
    case "$arg" in
        --skip-build) SKIP_BUILD=1 ;;
        --dry-run) DRY_RUN=1 ;;
        -*) echo "unknown option: $arg" >&2; exit 2 ;;
        *)
            if [ -z "$VERSION_TAG" ]; then VERSION_TAG="$arg"; else TARGET_PLATFORM="$arg"; fi
            ;;
    esac
done

if ! [[ "$VERSION_TAG" =~ ^v?[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]]; then
    echo "Usage: $0 <version, e.g. 1.0.0> [platform] [--skip-build] [--dry-run]" >&2
    exit 2
fi

if [ ! -f Cargo.toml ] || [ ! -d assets ]; then
    echo "Run this script from the repository root." >&2
    exit 2
fi

if [ "$DRY_RUN" -eq 0 ] && ! command -v butler >/dev/null 2>&1; then
    echo "butler was not found on PATH (https://itch.io/docs/butler/)." >&2
    exit 2
fi

CHANNEL="$PROJECT:$TARGET_PLATFORM"
PACKAGE_DIR="./target/itch_dist/$TARGET_PLATFORM"
echo "Neon Veil // itch.io deployment: $CHANNEL @ $VERSION_TAG"

if [ "$SKIP_BUILD" -eq 0 ]; then
    cargo build --release --locked --bin game_client
fi

if [ ! -f ./target/release/game_client ]; then
    echo "Client binary not found at ./target/release/game_client." >&2
    exit 1
fi

rm -rf "$PACKAGE_DIR"
mkdir -p "$PACKAGE_DIR"
cp ./target/release/game_client "$PACKAGE_DIR/NeonVeil"
cp -r ./assets "$PACKAGE_DIR/assets"
chmod +x "$PACKAGE_DIR/NeonVeil"

if [ "$DRY_RUN" -eq 1 ]; then
    echo "Dry run: would run butler push $PACKAGE_DIR $CHANNEL --userversion $VERSION_TAG"
    exit 0
fi

butler push "$PACKAGE_DIR" "$CHANNEL" --userversion "$VERSION_TAG"
echo "Deployment to $CHANNEL ($VERSION_TAG) complete."
