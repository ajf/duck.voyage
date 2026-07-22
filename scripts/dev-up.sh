#!/usr/bin/env bash
# Local dev stack via podman (no docker/compose on this machine):
# Postgres 16 (:5432), MinIO (:9000 API / :9001 console), Keycloak (:8081).
# Everything is local; nothing here touches any deployment target.
set -euo pipefail

cd "$(dirname "$0")/.."

POD=duck-dev

if ! podman pod exists "$POD"; then
    podman pod create --name "$POD" \
        -p 5432:5432 -p 9000:9000 -p 9001:9001 -p 8081:8080
fi

for vol in duck-pg duck-minio duck-kc; do
    podman volume create --ignore "$vol" >/dev/null
done

podman run -d --pod "$POD" --name duck-pg \
    -e POSTGRES_USER=duck -e POSTGRES_PASSWORD=duck -e POSTGRES_DB=duck \
    -v duck-pg:/var/lib/postgresql/data \
    docker.io/library/postgres:16 >/dev/null

podman run -d --pod "$POD" --name duck-minio \
    -e MINIO_ROOT_USER=minioadmin -e MINIO_ROOT_PASSWORD=minioadmin \
    -v duck-minio:/data \
    docker.io/minio/minio:latest server /data --console-address ":9001" >/dev/null

podman run -d --pod "$POD" --name duck-kc \
    -e KC_BOOTSTRAP_ADMIN_USERNAME=admin -e KC_BOOTSTRAP_ADMIN_PASSWORD=admin \
    -v duck-kc:/opt/keycloak/data \
    -v ./dev/keycloak-realm.json:/opt/keycloak/data/import/ducks-realm.json:ro,z \
    docker.io/keycloak/keycloak:latest start-dev --import-realm >/dev/null

echo -n "waiting for postgres"
until podman exec duck-pg pg_isready -U duck -d duck -q 2>/dev/null; do
    echo -n .
    sleep 0.5
done
echo " up"

# Create the photo bucket (idempotent).
podman exec duck-minio mc alias set local http://localhost:9000 minioadmin minioadmin >/dev/null
podman exec duck-minio mc mb --ignore-existing local/duck-photos >/dev/null

echo "postgres  postgres://duck:duck@localhost:5432/duck"
echo "minio     http://localhost:9000 (console :9001, minioadmin/minioadmin)"
echo "keycloak  http://localhost:8081 (admin/admin; realm 'ducks' imported)"
