#!/bin/sh
# Starts the whole production compose from images built from this checkout, in
# a throwaway project, and fails unless every service becomes healthy.
#
# Run through `make prod-check`. What it exercises is what nothing else does:
# the runtime images, the users, read-only filesystems and capability sets of
# docker-compose.yml, and every healthcheck in it.
#
# Nothing it creates outlives it. The project, its containers, its volumes, its
# network, the two images it built and the temporary directory are removed on
# exit, failure included. It touches no other container or volume, and pushes
# nothing anywhere.
#
# Two differences from a real deployment, both forced by running on a
# developer machine:
#   - The Garage volumes are named volumes, handed to uid 1001 the way the
#     README tells an operator to prepare /data/pwnforge/garage, instead of
#     bind mounts on directories that do not exist here.
#   - Garage reads a configuration generated with fresh secrets, instead of the
#     gitignored ops/garage/garage.toml.

set -eu

PROJECT=pwnforge-prodcheck
ROOT=$(cd "$(dirname "$0")/.." && pwd)
BACKEND_IMAGE=pwnforge-prodcheck-backend:local
FRONTEND_IMAGE=pwnforge-prodcheck-frontend:local
TIMEOUT_SECS=${PRODCHECK_TIMEOUT_SECS:-300}
SERVICES="db redis garage backend frontend"

TMP=$(mktemp -d)

# Compose runs with an empty environment plus what Docker itself needs: a
# variable exported in the caller's shell would otherwise take precedence over
# the generated values below.
compose() {
    env -i PATH="$PATH" HOME="$HOME" \
        ${DOCKER_HOST:+DOCKER_HOST="$DOCKER_HOST"} \
        ${DOCKER_CONFIG:+DOCKER_CONFIG="$DOCKER_CONFIG"} \
        ${DOCKER_CONTEXT:+DOCKER_CONTEXT="$DOCKER_CONTEXT"} \
        ${XDG_RUNTIME_DIR:+XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR"} \
        docker compose -p "$PROJECT" \
        -f "$ROOT/docker-compose.yml" -f "$TMP/override.yml" \
        --env-file "$TMP/env" --profile garage "$@"
}

cleanup() {
    status=$?
    trap - EXIT INT TERM
    echo "prod-check: cleaning up"
    # Anonymous volumes (redis's /data) are collected from this project's own
    # containers before they go: `down -v` does not always remove them.
    anonymous=$(docker ps -aq --filter "label=com.docker.compose.project=$PROJECT" |
        xargs -r docker inspect -f '{{range .Mounts}}{{if eq .Type "volume"}}{{.Name}} {{end}}{{end}}')
    compose down -v --remove-orphans >/dev/null 2>&1 || true
    for volume in $anonymous; do
        docker volume rm "$volume" >/dev/null 2>&1 || true
    done
    docker image rm "$BACKEND_IMAGE" "$FRONTEND_IMAGE" >/dev/null 2>&1 || true
    rm -rf "$TMP"
    exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

hex() { od -An -tx1 -N"$1" /dev/urandom | tr -d ' \n'; }

echo "prod-check: building $BACKEND_IMAGE and $FRONTEND_IMAGE"
docker build -q -t "$BACKEND_IMAGE" "$ROOT/backend" >/dev/null
docker build -q -t "$FRONTEND_IMAGE" "$ROOT/frontend" >/dev/null

# Test values for every variable docker-compose.yml requires. PORT is not one:
# the compose file fixes it to 3000, the port frontend/nginx.conf proxies to.
HTTP_PORT=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
cat > "$TMP/env" <<EOF
POSTGRES_USER=prodcheck
POSTGRES_PASSWORD=$(hex 24)
POSTGRES_DB=prodcheck
REDIS_PASSWORD=$(hex 24)
JWT_SECRET=$(hex 32)
ENCRYPTION_KEY=$(hex 32)
PUBLIC_URL=http://127.0.0.1:$HTTP_PORT
CORS_ORIGIN=http://127.0.0.1:$HTTP_PORT
HTTP_PORT=127.0.0.1:$HTTP_PORT
PWNFORGE_S3_ACCESS_KEY_ID=GK$(hex 12)
PWNFORGE_S3_SECRET_ACCESS_KEY=$(hex 32)
PWNFORGE_S3_BUCKET_ATTACHMENTS=prodcheck-attachments
PWNFORGE_S3_BUCKET_AVATARS=prodcheck-avatars
EOF

sed -e "s/^rpc_secret = .*/rpc_secret = \"$(hex 32)\"/" \
    -e "s/^admin_token = .*/admin_token = \"$(hex 32)\"/" \
    "$ROOT/ops/garage/garage.toml.example" > "$TMP/garage.toml"
chmod 0644 "$TMP/garage.toml"

cat > "$TMP/override.yml" <<EOF
services:
  backend:
    image: $BACKEND_IMAGE
  frontend:
    image: $FRONTEND_IMAGE
  garage:
    volumes: !override
      - $TMP/garage.toml:/etc/garage.toml:ro
      - garage-meta:/var/lib/garage/meta
      - garage-data:/var/lib/garage/data
  garage-init:
    volumes: !override
      - $ROOT/ops/garage/bootstrap.py:/init/bootstrap.py:ro
      - $TMP/garage.toml:/etc/garage.toml:ro
volumes:
  garage-meta:
    driver_opts: !reset {}
  garage-data:
    driver_opts: !reset {}
EOF

echo "prod-check: creating project $PROJECT"
compose up --no-start --quiet-pull >/dev/null 2>&1

# The step the README asks of an operator, applied to the named volumes.
docker run --rm --network none \
    -v "${PROJECT}_garage-meta:/meta" -v "${PROJECT}_garage-data:/data" \
    python:3.13-alpine chown 1001:1001 /meta /data

echo "prod-check: provisioning object storage"
compose up -d --quiet-pull garage >/dev/null 2>&1
compose up --quiet-pull --exit-code-from garage-init garage-init >/dev/null 2>&1 || {
    echo "prod-check: garage-init failed"
    compose logs garage-init | tail -20
    exit 1
}

echo "prod-check: starting every service"
compose up -d --quiet-pull >/dev/null 2>&1

health() {
    container=$(compose ps -q "$1")
    [ -n "$container" ] || { echo missing; return; }
    docker inspect -f '{{if .State.Health}}{{.State.Health.Status}}{{else}}no-healthcheck{{end}}' "$container"
}

deadline=$(($(date +%s) + TIMEOUT_SECS))
while :; do
    pending=""
    for service in $SERVICES; do
        [ "$(health "$service")" = healthy ] || pending="$pending $service"
    done
    [ -z "$pending" ] && break
    if [ "$(date +%s)" -ge "$deadline" ]; then
        break
    fi
    sleep 3
done

echo "prod-check: state after $(( TIMEOUT_SECS - (deadline - $(date +%s)) ))s"
failed=0
for service in $SERVICES; do
    state=$(health "$service")
    printf '  %-9s %s\n' "$service" "$state"
    [ "$state" = healthy ] || failed=1
done
printf '  %-9s %s\n' garage-init "exited $(docker inspect -f '{{.State.ExitCode}}' "$(compose ps -aq garage-init)")"

if [ "$failed" -ne 0 ]; then
    for service in $SERVICES; do
        [ "$(health "$service")" = healthy ] && continue
        echo "prod-check: last log lines of $service"
        compose logs --tail 15 "$service" 2>&1 | sed 's/^/    /'
    done
    echo "prod-check: FAILED"
    exit 1
fi

echo "prod-check: every service is healthy"
