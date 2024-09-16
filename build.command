#!/bin/bash
cd -- "$(dirname "$BASH_SOURCE")"
rm -r "Train Ute Model.app"
pushd train-ui
git pull
cargo tauri build
popd
cp -fr "./target/release/bundle/macos/Train Ute Model.app" "Train Ute Model.app"
