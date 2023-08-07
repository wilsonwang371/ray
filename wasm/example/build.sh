#!/bin/bash

set -e

WASI_VERSION=14
WASI_VERSION_FULL=${WASI_VERSION}.0

# get current dir
DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"

# find out the operating system: darwin, linux, etc.
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
if [ "${OS}" = "darwin" ]; then
    export OS="macos"
fi

function install_sdk() {
    if ! [ -x "$(command -v wget)" ]; then
        echo "Error: wget is not installed." >&2
        exit 1
    fi

    if ! [ -x "$(command -v tar)" ]; then
        echo "Error: tar is not installed." >&2
        exit 1
    fi

    wget https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-${WASI_VERSION}/wasi-sdk-${WASI_VERSION_FULL}-${OS}.tar.gz
    rm -rf wasi-sdk-${WASI_VERSION_FULL}
    tar xf wasi-sdk-${WASI_VERSION_FULL}-${OS}.tar.gz
}

# /opt/wasi-sdk/bin/clang --target=wasm32-unknown-wasi \
#     --sysroot /opt/wasi-sdk/share/wasi-sysroot/ \
#     -Wl,--export-all \
#     ./simple.c -o simple.wasm

WASI_SDK_PATH=$(pwd -P)/wasi-sdk-${WASI_VERSION_FULL}

# check if wasi clang is installed
if ! command -v "${WASI_SDK_PATH}/bin/clang" >/dev/null 2>&1; then
    echo "Installing WASI SDK..."
    install_sdk
    if [ ! -d "${WASI_SDK_PATH}" ]; then
        echo "Error: WASI SDK not installed." >&2
        exit 1
    fi
fi

export CC="${WASI_SDK_PATH}/bin/clang --sysroot=${WASI_SDK_PATH}/share/wasi-sysroot"

# iterate through all c files in the current directory recursively
for file in $(find ${DIR} -name "*.c" | grep -v "wasi-sdk" ); do
    echo "Compiling ${file}..."
    $CC --target=wasm32-unknown-wasi \
        -Wno-format-security \
        -Wl,--export-all \
        -I${DIR}/../include \
        -I${file%/*}/ \
        ${file} -o ${file%.c}.wasm # -nostdlib

    # check if wabt is installed
    if ! [ -x "$(command -v wasm2wat)" ]; then
        # warn if not installed
        echo "Warning: wabt is not installed. Please install wabt to generate wat files." >&2
    else
        # if installed, generate wat files
        wasm2wat ${file%.c}.wasm  -o ${file%.c}.wat
        wasm-objdump -dhs ${file%.c}.wasm > ${file%.c}.wasm.dump
    fi
done
