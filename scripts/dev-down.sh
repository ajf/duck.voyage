#!/usr/bin/env bash
# Tear down the local dev stack. Volumes survive unless --wipe is given.
set -euo pipefail

podman pod rm -f duck-dev 2>/dev/null || true

if [[ "${1:-}" == "--wipe" ]]; then
    podman volume rm -f duck-pg duck-minio duck-kc 2>/dev/null || true
    echo "volumes wiped"
fi
