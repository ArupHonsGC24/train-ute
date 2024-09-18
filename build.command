#!/bin/bash
cd -- "$(dirname "$BASH_SOURCE")" || exit
rm -r "Train Ute Model.app"
pushd train-ui || exit
git pull
cargo tauri build
popd || exit
cp -fr "./target/release/bundle/macos/Train Ute Model.app" "Train Ute Model.app"
