#!/bin/bash

INPUT_PATH="target/wasm32-unknown-unknown/release/rust_wasm_guest.wasm"
OUTPUT_PATH="rust_wasm_guest.wasm"

cargo build --target wasm32-unknown-unknown --release && \
    wasm-opt -Oz -o $OUTPUT_PATH $INPUT_PATH && \
    wasm-strip $OUTPUT_PATH
