#!/bin/bash
cd -- "$(dirname "$BASH_SOURCE")" || exit
rm -r "Train Ute Model.app"
pushd train-ui || exit
git pull --recurse-submodules
cargo tauri build -b app
popd || exit
cp -fr "./target/release/bundle/macos/Train Ute Model.app" "Train Ute Model.app"
