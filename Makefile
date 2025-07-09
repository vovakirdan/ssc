.PHONY: dev, build

all: dev, build

build:
	npm run tauri build

build-ios:
	npm run tauri ios build

build-android-apk:
	npm run tauri android build -- --apk

dev:
	npm run tauri dev

run-appimage:
	./src-tauri/target/release/bundle/appimage/ssc_0.1.0_amd64.AppImage

run-dev:
	npm run tauri dev
