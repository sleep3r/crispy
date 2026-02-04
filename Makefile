.PHONY: help install dev build clean check test fmt lint

# Default target
.DEFAULT_GOAL := help

# Colors for output
BLUE := \033[0;34m
GREEN := \033[0;32m
YELLOW := \033[0;33m
NC := \033[0m # No Color

help: ## Show help for commands
	@echo "$(BLUE)Crispy - Tauri + React + Rust$(NC)"
	@echo ""
	@echo "$(GREEN)Available commands:$(NC)"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  $(YELLOW)%-15s$(NC) %s\n", $$1, $$2}'

install: ## Install all dependencies (Node.js and Rust)
	@echo "$(BLUE)Installing Node.js dependencies...$(NC)"
	npm install
	@echo "$(BLUE)Checking Rust...$(NC)"
	@command -v rustc >/dev/null 2>&1 || { echo "$(YELLOW)⚠️  Rust is not installed. Install via rustup: https://rustup.rs/$(NC)"; exit 1; }
	@echo "$(GREEN)✅ Dependencies installed$(NC)"

dev: ## Run application in development mode
	@echo "$(BLUE)Starting in development mode...$(NC)"
	npm run tauri dev

build: ## Build application for the current platform
	@echo "$(BLUE)Building application...$(NC)"
	@unset CI && npm run tauri build
	@echo "$(BLUE)Signing application...$(NC)"
	@codesign --sign - --force --deep target/aarch64-apple-darwin/release/bundle/macos/Crispy.app
	@echo "$(GREEN)✅ Build complete:$(NC)"
	@echo "   • App: target/aarch64-apple-darwin/release/bundle/macos/Crispy.app"
	@echo "   • DMG: target/aarch64-apple-darwin/release/bundle/dmg/Crispy_*.dmg"

build-dev: ## Build application in development mode (debug)
	@echo "$(BLUE)Building application (debug)...$(NC)"
	cd src-tauri && cargo build
	@echo "$(GREEN)✅ Debug build complete$(NC)"

clean: ## Clean build artifacts
	@echo "$(BLUE)Cleaning artifacts...$(NC)"
	rm -rf node_modules
	rm -rf dist
	rm -rf src-tauri/target
	@echo "$(GREEN)✅ Cleanup complete$(NC)"

clean-cache: ## Clean cache only (keep node_modules and target)
	@echo "$(BLUE)Cleaning cache...$(NC)"
	rm -rf dist
	rm -rf src-tauri/target/debug
	rm -rf src-tauri/target/release
	@echo "$(GREEN)✅ Cache cleaned$(NC)"

check: ## Check code (TypeScript and Rust)
	@echo "$(BLUE)Checking TypeScript...$(NC)"
	npm run build --dry-run 2>/dev/null || npx tsc --noEmit
	@echo "$(BLUE)Checking Rust...$(NC)"
	cd src-tauri && cargo check
	@echo "$(GREEN)✅ Check complete$(NC)"

fmt: ## Format code (Rust)
	@echo "$(BLUE)Formatting Rust code...$(NC)"
	cd src-tauri && cargo fmt
	@echo "$(GREEN)✅ Formatting complete$(NC)"

lint: ## Run linters
	@echo "$(BLUE)Linting TypeScript...$(NC)"
	npx eslint src/ --ext .ts,.tsx || echo "$(YELLOW)⚠️  ESLint not configured$(NC)"
	@echo "$(BLUE)Linting Rust...$(NC)"
	cd src-tauri && cargo clippy || echo "$(YELLOW)⚠️  Clippy not installed$(NC)"

test: ## Run tests (if available)
	@echo "$(BLUE)Running tests...$(NC)"
	@if [ -f "package.json" ] && grep -q '"test"' package.json; then \
		npm test; \
	else \
		echo "$(YELLOW)⚠️  Tests not configured$(NC)"; \
	fi

update: ## Update dependencies
	@echo "$(BLUE)Updating Node.js dependencies...$(NC)"
	npm update
	@echo "$(BLUE)Updating Rust dependencies...$(NC)"
	cd src-tauri && cargo update
	@echo "$(GREEN)✅ Dependencies updated$(NC)"

version-bump: ## Bump version (TYPE=major|minor|patch, default patch)
	@echo "$(BLUE)Updating version...$(NC)"
	@TYPE=$(if $(TYPE),$(TYPE),patch); \
	CURRENT=$$(grep '"version":' package.json | head -1 | sed 's/.*"version": "\(.*\)".*/\1/'); \
	MAJOR=$$(echo $$CURRENT | cut -d. -f1); \
	MINOR=$$(echo $$CURRENT | cut -d. -f2); \
	PATCH=$$(echo $$CURRENT | cut -d. -f3); \
	if [ "$$TYPE" = "major" ]; then \
		MAJOR=$$(($$MAJOR + 1)); MINOR=0; PATCH=0; \
	elif [ "$$TYPE" = "minor" ]; then \
		MINOR=$$(($$MINOR + 1)); PATCH=0; \
	else \
		PATCH=$$(($$PATCH + 1)); \
	fi; \
	NEW_VERSION="$$MAJOR.$$MINOR.$$PATCH"; \
	echo "$(YELLOW)$$CURRENT$(NC) → $(GREEN)$$NEW_VERSION$(NC)"; \
	sed -i.bak -E "s/\"version\": \"[^\"]+\"/\"version\": \"$$NEW_VERSION\"/" package.json && rm package.json.bak; \
	sed -i.bak -E "s/^version = \"[^\"]+\"/version = \"$$NEW_VERSION\"/" src-tauri/Cargo.toml && rm src-tauri/Cargo.toml.bak; \
	sed -i.bak -E "s/\"version\": \"[^\"]+\"/\"version\": \"$$NEW_VERSION\"/" src-tauri/tauri.conf.json && rm src-tauri/tauri.conf.json.bak; \
	echo "$(GREEN)✅ Version updated in all files:$(NC)"; \
	echo "   • package.json"; \
	echo "   • src-tauri/Cargo.toml"; \
	echo "   • src-tauri/tauri.conf.json"

version: ## Show current version
	@VERSION=$$(grep '"version":' package.json | head -1 | sed 's/.*"version": "\(.*\)".*/\1/'); \
	echo "$(GREEN)Current version: $$VERSION$(NC)"

# Short aliases
i: install ## Alias for install
d: dev ## Alias for dev
b: build ## Alias for build
c: clean ## Alias for clean
v: version ## Alias for version

