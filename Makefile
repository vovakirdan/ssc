.PHONY: all dev build build-ios build-android build-android-apk build-android-aab install install-linux install-windows install-macos launch launch-linux launch-windows launch-macos check check-cargo check-frontend fmt fmt-cargo fmt-frontend clean

# Основные команды
all: dev build

# Команды разработки
dev:
	npm run tauri dev

run-dev:
	npm run tauri dev

# Команды сборки
build:
	npm run tauri build

build-ios:
	npm run tauri ios build

build-android:
	npm run tauri android build

build-android-apk:
	npm run tauri android build -- --apk

build-android-aab:
	npm run tauri android build -- --aab

# Команды установки (в зависимости от ОС)
install: install-$(shell uname -s | tr '[:upper:]' '[:lower:]')

install-linux: build
	@echo "Установка для Linux..."
	@if command -v apt-get >/dev/null 2>&1; then \
		sudo dpkg -i src-tauri/target/release/bundle/deb/*.deb; \
	elif command -v dnf >/dev/null 2>&1; then \
		sudo dnf install src-tauri/target/release/bundle/rpm/*.rpm; \
	elif command -v yum >/dev/null 2>&1; then \
		sudo yum install src-tauri/target/release/bundle/rpm/*.rpm; \
	elif command -v pacman >/dev/null 2>&1; then \
		sudo pacman -U src-tauri/target/release/bundle/pacman/*.pkg.tar.zst; \
	else \
		echo "Неизвестный пакетный менеджер. Установите вручную из src-tauri/target/release/bundle/"; \
	fi

install-windows: build
	@echo "Установка для Windows..."
	@if [ -f "src-tauri/target/release/bundle/msi/*.msi" ]; then \
		msiexec /i src-tauri/target/release/bundle/msi/*.msi; \
	elif [ -f "src-tauri/target/release/bundle/nsis/*.exe" ]; then \
		./src-tauri/target/release/bundle/nsis/*.exe; \
	else \
		echo "Установочные файлы не найдены в src-tauri/target/release/bundle/"; \
	fi

install-macos: build
	@echo "Установка для macOS..."
	@if [ -f "src-tauri/target/release/bundle/dmg/*.dmg" ]; then \
		hdiutil attach src-tauri/target/release/bundle/dmg/*.dmg; \
		cp -R /Volumes/*/ssc.app /Applications/; \
		hdiutil detach /Volumes/*; \
	elif [ -f "src-tauri/target/release/bundle/app/*.app" ]; then \
		cp -R src-tauri/target/release/bundle/app/*.app /Applications/; \
	else \
		echo "Установочные файлы не найдены в src-tauri/target/release/bundle/"; \
	fi

# Команды запуска (в зависимости от ОС)
launch: launch-$(shell uname -s | tr '[:upper:]' '[:lower:]')

launch-linux:
	@echo "Запуск приложения на Linux..."
	@if command -v ssc >/dev/null 2>&1; then \
		ssc; \
	elif [ -f "/usr/local/bin/ssc" ]; then \
		/usr/local/bin/ssc; \
	elif [ -f "./src-tauri/target/release/ssc" ]; then \
		./src-tauri/target/release/ssc; \
	else \
		echo "Приложение не найдено. Сначала выполните make install"; \
	fi

launch-windows:
	@echo "Запуск приложения на Windows..."
	@if [ -f "C:/Program Files/ssc/ssc.exe" ]; then \
		"C:/Program Files/ssc/ssc.exe"; \
	elif [ -f "C:/Program Files (x86)/ssc/ssc.exe" ]; then \
		"C:/Program Files (x86)/ssc/ssc.exe"; \
	elif [ -f "./src-tauri/target/release/ssc.exe" ]; then \
		./src-tauri/target/release/ssc.exe; \
	else \
		echo "Приложение не найдено. Сначала выполните make install"; \
	fi

launch-macos:
	@echo "Запуск приложения на macOS..."
	@if [ -d "/Applications/ssc.app" ]; then \
		open /Applications/ssc.app; \
	elif [ -f "./src-tauri/target/release/ssc" ]; then \
		./src-tauri/target/release/ssc; \
	else \
		echo "Приложение не найдено. Сначала выполните make install"; \
	fi

# Команды проверки
check: check-cargo check-frontend

check-cargo:
	@echo "Проверка Rust кода..."
	cd src-tauri && cargo check && cd ..

check-frontend:
	@echo "Проверка TypeScript кода..."
	npm run type-check

# Команды форматирования
fmt: fmt-cargo fmt-frontend

fmt-cargo:
	@echo "Форматирование Rust кода..."
	cd src-tauri && cargo fmt && cd ..

fmt-frontend:
	@echo "Форматирование TypeScript кода..."
	npm run format

# Команды очистки
clean:
	@echo "Очистка проекта..."
	cd src-tauri && cargo clean && cd ..
	rm -rf node_modules
	rm -rf dist
	rm -rf .tauri

# Дополнительные команды
run-appimage:
	./src-tauri/target/release/bundle/appimage/ssc_0.1.0_amd64.AppImage

# Команда для быстрого тестирования
test: check
	@echo "Запуск тестов..."
	cd src-tauri && cargo test
	npm test
