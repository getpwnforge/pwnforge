# Makefile for PwnForge project
.PHONY: help up down migrate seed reset-db reset-storage test lint clean

# Set the default goal to 'help' if no target is specified
.DEFAULT_GOAL := help

COMPOSE_FILE := docker-compose.dev.yml

## up: Start the development environment (builds images if needed)
up:
	docker compose -f $(COMPOSE_FILE) up -d

## down: Stop and remove containers (keeps volumes)
down:
	docker compose -f $(COMPOSE_FILE) down

## migrate: Apply pending migrations to the database
migrate:
	cargo run --bin migration --manifest-path backend/Cargo.toml

## seed: Populate the database with test data
seed:
	cargo run --bin seed --manifest-path backend/Cargo.toml

## reset-db: Destroy and recreate the development database (DESTRUCTIVE)
#
# The dev database drifts for one specific reason: migrations on an unreleased
# branch get edited in place, and a database that already applied the old
# version never sees the change - the row in `seaql_migrations` says it is
# applied. The failure then surfaces as a missing column in an unrelated test,
# which is a slow thing to work out from scratch. This is the shortcut.
#
# Destroys every row in the dev database. Volumes only, nothing outside Docker.
reset-db:
	@echo "This destroys the development database. Ctrl-C within 3s to abort."
	@sleep 3
	docker compose -f $(COMPOSE_FILE) rm -sf db
	docker volume rm -f pwnforge_db-data
	docker compose -f $(COMPOSE_FILE) up -d db
	@echo "Waiting for Postgres to accept connections..."
	@until docker compose -f $(COMPOSE_FILE) exec -T db pg_isready -q 2>/dev/null; do sleep 1; done
	@echo "Database recreated. Migrations run on the next backend start, or: make migrate"

## reset-storage: Destroy and re-provision the bundled Garage node (DESTRUCTIVE)
#
# Wipes every stored object and every bucket, then re-runs the bootstrap.
# Refuses unless PWNFORGE_S3_ENDPOINT is a local address, so an operator whose
# .env points at their real S3 cannot lose it to a command named "reset".
# Override with ALLOW_REMOTE_RESET=1. The guard and its reasoning live in
# ops/scripts/reset-storage.sh.
reset-storage:
	COMPOSE_FILE=$(COMPOSE_FILE) ./ops/scripts/reset-storage.sh

## test: Launch the test suite for backend and frontend
test:
	@echo "Running backend tests..."
	cargo test --all --manifest-path backend/Cargo.toml
	@echo "Running frontend tests..."
	cd frontend && pnpm test

## lint: Check the code (clippy + eslint/tsc)
lint:
	@echo "Running backend linting..."
	cargo clippy --all-targets --all-features --manifest-path backend/Cargo.toml -- -D warnings
	@echo "Running frontend linting..."
	cd frontend && pnpm lint && pnpm tsc --noEmit


## clean: Remove build artifacts and temporary files
clean:
	docker compose -f $(COMPOSE_FILE) down --volumes --remove-orphans
	rm -rf backend/target frontend/dist frontend/node_modules

## help: Display this help
help:
	@grep -E '^## ' $(MAKEFILE_LIST) | sed 's/## //' | awk -F': ' '{printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'