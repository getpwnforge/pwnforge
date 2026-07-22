# Makefile for PwnForge project
.PHONY: help up down migrate seed test lint clean

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