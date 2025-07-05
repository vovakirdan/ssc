.PHONY: dev, build

all: dev, build

build:
	npm run tauri build

dev:
	npm run tauri dev

run-appimage:
	./src-tauri/target/release/bundle/appimage/ssc_0.1.0_amd64.AppImage

run-dev:
	npm run tauri dev
