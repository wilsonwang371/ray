#!/bin/bash

/opt/wasi-sdk/bin/clang --sysroot /opt/wasi-sdk/share/wasi-sysroot/ -Wl,--export-all ./test.c -o test.wasm
wasm2wat test.wasm -o test.wat
wasm-objdump -dhs test.wasm > test.objdump
