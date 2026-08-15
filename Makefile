# Casivell — convenience targets
#
# This Makefile wires together the Rust workspace and the React web front end.
# It assumes a working Rust toolchain (see rust-toolchain.toml) and Node/npm.

SHELL := bash
.SHELLFLAGS := -euo pipefail -c

WEB_DIR := web-react

.PHONY: help
help: ## Show this help
	@grep -E '^[a-zA-Z0-9_-]+:.*##' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*##"}; {printf "  %-20s %s\n", $$1, $$2}'

.PHONY: check
check: ## Run the same checks as CI
	cargo test --workspace
	cargo clippy --workspace --all-targets -- -D warnings
	cargo build --workspace --target wasm32-unknown-unknown --release
	python3 scripts/check_no_statutory_literals.py
	python3 docs/reference/generate_tariff_reference.py

.PHONY: backend backend-release backend-cli
backend: ## Build the backend / CLI and WASM artifact (debug)
	cargo build --workspace --release

backend-release: ## Build the backend / CLI and WASM artifact (release)
	cargo build --workspace --release --target wasm32-unknown-unknown

backend-cli: ## Build and run the CLI with example payslip arguments
	cargo run -p casivell-cli -- --gross 4500 --class 1

.PHONY: wasm-copy wasm-build
wasm-copy: ## Copy the already-built release WASM into web-react/public/
	cd $(WEB_DIR) && npm run copy-wasm

wasm-build: ## Build the WASM release artifact and copy it into web-react/public/
	cd $(WEB_DIR) && npm run build:wasm

.PHONY: frontend frontend-deps frontend-dev frontend-build frontend-preview
frontend-deps: ## Install npm dependencies for the React app
	cd $(WEB_DIR) && npm install

frontend: frontend-deps wasm-copy frontend-dev ## Full frontend start: install, copy wasm, run dev server

frontend-dev: ## Start the Vite dev server for the React app
	cd $(WEB_DIR) && npm run dev

frontend-build: ## Type-check and build the production React bundle
	cd $(WEB_DIR) && npm run build

frontend-preview: ## Serve the built React bundle from web-react/dist/
	cd $(WEB_DIR) && npm run preview

.PHONY: start
start: backend frontend ## Build backend and start frontend dev server (default target)

.PHONY: clean
clean: ## Remove Rust and frontend build artifacts
	cargo clean
	rm -rf $(WEB_DIR)/node_modules $(WEB_DIR)/dist $(WEB_DIR)/.vite
