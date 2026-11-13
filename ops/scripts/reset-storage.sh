#!/bin/sh
# Destroys the bundled object store's volumes and re-provisions it.
#
# Called by `make reset-storage`. Lives in a script rather than inline in the
# Makefile because the guard below needs real control flow, and a Makefile
# recipe is a poor place to put logic that decides whether to delete data.
#
# THE GUARD
# ---------
# This is a development command whose name says "reset", and .env is the same
# file an operator points at their real S3. An operator running against their
# own R2 or AWS bucket who types `make reset-storage` must not be able to
# destroy it by accident.
#
# The volumes removed below are local either way, so nothing remote is deleted
# by this script directly. The danger is the other half: the bootstrap that
# follows re-provisions whatever PWNFORGE_S3_ENDPOINT names, and an operator
# who watched a "reset" succeed will reasonably believe the command was scoped
# to their machine. Refusing up front keeps that belief true.
#
# Override with ALLOW_REMOTE_RESET=1 for a deliberate non-local target.

set -eu

COMPOSE_FILE="${COMPOSE_FILE:-docker-compose.dev.yml}"
ENV_FILE="${ENV_FILE:-.env}"

if [ ! -f "${ENV_FILE}" ]; then
    echo "reset-storage: ${ENV_FILE} not found; run this from the repository root" >&2
    exit 1
fi

endpoint=$(sed -n 's/^PWNFORGE_S3_ENDPOINT=//p' "${ENV_FILE}" | tail -1 | tr -d '"'"'" | tr -d "'")

if [ -z "${endpoint}" ]; then
    # An empty endpoint is not "unconfigured": it is how .env.example spells
    # "real AWS S3", so it is the most dangerous value of all here.
    echo "reset-storage: PWNFORGE_S3_ENDPOINT is unset or empty in ${ENV_FILE}." >&2
    echo "  An empty endpoint means real AWS S3. Refusing." >&2
    exit 1
fi

case "${endpoint}" in
    http://localhost:*|http://127.0.0.1:*|http://[::1]:*|http://garage:*|http://minio:*)
        ;;
    *)
        if [ "${ALLOW_REMOTE_RESET:-}" != "1" ]; then
            echo "reset-storage: refusing, PWNFORGE_S3_ENDPOINT is not a local address:" >&2
            echo "    ${endpoint}" >&2
            echo "  This target destroys buckets. If you genuinely mean it:" >&2
            echo "    ALLOW_REMOTE_RESET=1 make reset-storage" >&2
            exit 1
        fi
        echo "reset-storage: ALLOW_REMOTE_RESET=1 is set, proceeding against ${endpoint}"
        ;;
esac

echo "This destroys all stored objects at ${endpoint}. Ctrl-C within 3s to abort."
sleep 3

docker compose -f "${COMPOSE_FILE}" rm -sf garage
docker volume rm -f pwnforge_garage-meta pwnforge_garage-data
docker compose -f "${COMPOSE_FILE}" up -d garage
docker compose -f "${COMPOSE_FILE}" up garage-init
