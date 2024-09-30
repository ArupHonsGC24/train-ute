#!/bin/bash
cd -- "$(dirname "$BASH_SOURCE")" || exit

# installs rustup
xcode-select --install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# installs fnm (Fast Node Manager)
curl -fsSL https://fnm.vercel.app/install | bash

# activate fnm
source ~/.bashrc

# download and install Node.js
fnm use --install-if-missing 22

# install tauri cli
cargo install tauri-cli --version "^2.0.0-rc" --locked