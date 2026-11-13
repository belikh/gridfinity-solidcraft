#!/bin/sh
# Build the plugin package: tests, wasm32 core module, component, zip.
set -e
cd "$(dirname "$0")/plugin"
~/.cargo/bin/cargo test --quiet
RUSTFLAGS="--remap-path-prefix=$HOME=~" ~/.cargo/bin/cargo build --release --target wasm32-unknown-unknown
~/.cargo/bin/wasm-tools component new target/wasm32-unknown-unknown/release/gridfinity.wasm -o ../package/gridfinity/plugin.wasm
cd ../package
rm -f gridfinity-*.zip
python3 -c "import zipfile; z=zipfile.ZipFile('gridfinity-0.1.0.zip','w',zipfile.ZIP_DEFLATED); [z.write('gridfinity/'+f) for f in ('plugin.json','plugin.wasm')]"
echo "package/gridfinity-0.1.0.zip"
