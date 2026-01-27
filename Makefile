.PHONY: help install dev build clean check test fmt lint

# Default target
.DEFAULT_GOAL := help

# Colors for output
BLUE := \033[0;34m
GREEN := \033[0;32m
YELLOW := \033[0;33m
NC := \033[0m # No Color

help: ## Показать справку по командам
	@echo "$(BLUE)Crispy - Tauri + React + Rust$(NC)"
	@echo ""
	@echo "$(GREEN)Доступные команды:$(NC)"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  $(YELLOW)%-15s$(NC) %s\n", $$1, $$2}'

install: ## Установить все зависимости (Node.js и Rust)
	@echo "$(BLUE)Установка зависимостей Node.js...$(NC)"
	npm install
	@echo "$(BLUE)Проверка Rust...$(NC)"
	@command -v rustc >/dev/null 2>&1 || { echo "$(YELLOW)⚠️  Rust не установлен. Установите через rustup: https://rustup.rs/$(NC)"; exit 1; }
	@echo "$(GREEN)✅ Зависимости установлены$(NC)"

dev: ## Запустить приложение в режиме разработки
	@echo "$(BLUE)Запуск в режиме разработки...$(NC)"
	npm run tauri dev

build: ## Собрать приложение для текущей платформы
	@echo "$(BLUE)Сборка приложения...$(NC)"
	npm run tauri build
	@echo "$(GREEN)✅ Сборка завершена. Результат в src-tauri/target/release/$(NC)"

build-dev: ## Собрать приложение в режиме разработки (debug)
	@echo "$(BLUE)Сборка приложения (debug)...$(NC)"
	cd src-tauri && cargo build
	@echo "$(GREEN)✅ Debug сборка завершена$(NC)"

clean: ## Очистить артефакты сборки
	@echo "$(BLUE)Очистка артефактов...$(NC)"
	rm -rf node_modules
	rm -rf dist
	rm -rf src-tauri/target
	@echo "$(GREEN)✅ Очистка завершена$(NC)"

clean-cache: ## Очистить только кэш (node_modules и target остаются)
	@echo "$(BLUE)Очистка кэша...$(NC)"
	rm -rf dist
	rm -rf src-tauri/target/debug
	rm -rf src-tauri/target/release
	@echo "$(GREEN)✅ Кэш очищен$(NC)"

check: ## Проверить код (TypeScript и Rust)
	@echo "$(BLUE)Проверка TypeScript...$(NC)"
	npm run build --dry-run 2>/dev/null || npx tsc --noEmit
	@echo "$(BLUE)Проверка Rust...$(NC)"
	cd src-tauri && cargo check
	@echo "$(GREEN)✅ Проверка завершена$(NC)"

fmt: ## Форматировать код (Rust)
	@echo "$(BLUE)Форматирование Rust кода...$(NC)"
	cd src-tauri && cargo fmt
	@echo "$(GREEN)✅ Форматирование завершено$(NC)"

lint: ## Запустить линтеры
	@echo "$(BLUE)Линтинг TypeScript...$(NC)"
	npx eslint src/ --ext .ts,.tsx || echo "$(YELLOW)⚠️  ESLint не настроен$(NC)"
	@echo "$(BLUE)Линтинг Rust...$(NC)"
	cd src-tauri && cargo clippy || echo "$(YELLOW)⚠️  Clippy не установлен$(NC)"

test: ## Запустить тесты (если есть)
	@echo "$(BLUE)Запуск тестов...$(NC)"
	@if [ -f "package.json" ] && grep -q '"test"' package.json; then \
		npm test; \
	else \
		echo "$(YELLOW)⚠️  Тесты не настроены$(NC)"; \
	fi

update: ## Обновить зависимости
	@echo "$(BLUE)Обновление зависимостей Node.js...$(NC)"
	npm update
	@echo "$(BLUE)Обновление зависимостей Rust...$(NC)"
	cd src-tauri && cargo update
	@echo "$(GREEN)✅ Зависимости обновлены$(NC)"

run: dev ## Алиас для dev

version-bump: ## Увеличить версию (TYPE=major|minor|patch, по умолчанию patch)
	@echo "$(BLUE)Обновление версии...$(NC)"
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
	sed -i.bak "s/\"version\": \"$$CURRENT\"/\"version\": \"$$NEW_VERSION\"/" package.json && rm package.json.bak; \
	sed -i.bak "s/version = \"$$CURRENT\"/version = \"$$NEW_VERSION\"/" src-tauri/Cargo.toml && rm src-tauri/Cargo.toml.bak; \
	sed -i.bak "s/\"version\": \"$$CURRENT\"/\"version\": \"$$NEW_VERSION\"/" src-tauri/tauri.conf.json && rm src-tauri/tauri.conf.json.bak; \
	echo "$(GREEN)✅ Версия обновлена во всех файлах:$(NC)"; \
	echo "   • package.json"; \
	echo "   • src-tauri/Cargo.toml"; \
	echo "   • src-tauri/tauri.conf.json"

version: ## Показать текущую версию
	@VERSION=$$(grep '"version":' package.json | head -1 | sed 's/.*"version": "\(.*\)".*/\1/'); \
	echo "$(GREEN)Текущая версия: $$VERSION$(NC)"

# Быстрые команды
i: install ## Алиас для install
d: dev ## Алиас для dev
b: build ## Алиас для build
c: clean ## Алиас для clean
v: version ## Алиас для version

