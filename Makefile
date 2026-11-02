# TurtleMeet developer commands. Run `make` or `make help` for the list.
# Settings come from .env (copy .env.example). Variables can be overridden: make dev-api RUST_LOG=debug

-include .env
export

WEB := apps/web
LIVEKIT_CONFIG := infrastructure/livekit/livekit.dev.yaml

.DEFAULT_GOAL := help
.PHONY: help setup env infra-up infra-down db-create dev dev-livekit dev-api dev-web \
        fmt lint typecheck test test-unit e2e check build clean

help: ## Show this help
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z_-]+:.*## / {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

# ---------- Setup ----------

env: ## Create .env from .env.example (won't overwrite)
	@test -f .env && echo ".env already exists" || (cp .env.example .env && echo "created .env")

setup: env ## Install Rust and web dependencies
	cargo fetch
	cd $(WEB) && npm ci

infra-up: ## Start Postgres + Redis in Docker
	docker compose up -d --wait postgres redis

infra-down: ## Stop Docker Postgres + Redis (data volume is kept)
	docker compose down

db-create: ## Create the test database (main DB must exist; Docker creates it)
	@psql "$(DATABASE_URL)" -tAc "SELECT 1 FROM pg_database WHERE datname = 'turtlemeet_test'" | grep -q 1 \
		&& echo "turtlemeet_test already exists" \
		|| psql "$(DATABASE_URL)" -c "CREATE DATABASE turtlemeet_test"

# ---------- Run ----------

dev: ## Run LiveKit, API and web together (Ctrl-C stops all)
	@$(MAKE) -j3 --no-print-directory dev-livekit dev-api dev-web

dev-livekit: ## Run LiveKit SFU in dev mode (needs `brew install livekit`)
	livekit-server --dev --bind 127.0.0.1 --config $(LIVEKIT_CONFIG)

dev-api: ## Run the Rust API (applies DB migrations on start)
	cargo run -p server

dev-web: ## Run the Next.js dev server on :3000
	cd $(WEB) && npm run dev

# ---------- Quality ----------

fmt: ## Format Rust code
	cargo fmt

lint: ## rustfmt check, clippy (deny warnings), ESLint
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings
	cd $(WEB) && npm run lint

typecheck: ## TypeScript type check
	cd $(WEB) && npm run typecheck

test-unit: ## Rust unit tests only (no services needed)
	cargo test --lib --bins

test: ## All Rust tests (needs Postgres, Redis and LiveKit running)
	cargo test

e2e: ## Browser E2E with real media (needs `make dev` running + Google Chrome)
	cd $(WEB) && npm run e2e

check: lint typecheck test ## Everything CI runs, except E2E

# ---------- Build ----------

build: ## Release build of API and web
	cargo build --release -p server
	cd $(WEB) && npm run build

clean: ## Remove build outputs
	cargo clean
	rm -rf $(WEB)/.next $(WEB)/e2e/screenshots
