#!/bin/zsh
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# This script is meant to be executed on MacOS (hence zsh use - to get associative arrays otherwise
# unavailable in the bundled bash version).
#
# Before running the script, you need to download Sui binaries to a freshly created directory
# (make sure that there are no other files in this directory). Sui binary must be a compressed
# tarball (with tgz extension) and its name has to have the following format:
#   sui-VERSION-SHA-PLATFORM.tgz
#
# where:
# - VERSION    is the Sui binary version (e.g., "v.1.37.0)
# - SHA        is first 10 characgers of commit sha for the version of Sui repo's main branch
#              that this binary was build from
# - PLATFORM   is on of the supported platforms (macos-arm64, macos-x86_64, ubuntu-x86_64, windows-x86_64)
set -e

usage() {
    SCRIPT_NAME=$(basename "$1")
    >&2 echo "Usage: $SCRIPT_NAME  -pkg|-pub [-h] BINDIR"
    >&2 echo ""
    >&2 echo "Options:"
    >&2 echo " -pub          Publish extensions for all targets"
    >&2 echo " -pkg          Package extensions for all targets"
    >&2 echo " -h            Print this message"
    >&2 echo " BINDIR        Directory containing pre-built Sui binaries"
}

clean_tmp_dir() {
  test -d "$TMP_DIR" && rm -fr "$TMP_DIR"
}

if [[ "$@" == "" ]]; then
    usage $0
    exit 1
fi

BIN_DIR=""
for cmd in "$@"
do
    if [[ "$cmd" == "-h" ]]; then
        usage $0
        exit 0
    elif [[ "$cmd" == "-pkg" ]]; then
        OP="package"
        OPTS="-omove-VSCODE_OS.vsix"
    elif [[ "$cmd" == "-pub" ]]; then
        OP="publish"
        OPTS=""
    else
        BIN_DIR=$cmd

        if [[ ! -d "$BIN_DIR" ]]; then
            echo Sui binary directory $BIN_DIR does not exist
            usage $0
            exit 1
        fi
    fi
done

if [[ $BIN_DIR == "" ]]; then
    # directory storing Sui binaries have not been defined
    usage $0
    exit 1
fi

# a map from os version identifiers in Sui's binary distribution to os version identifiers
# representing VSCode's target platforms used for creating platform-specific plugin distributions
declare -A SUPPORTED_OS
SUPPORTED_OS[macos-arm64]=darwin-arm64
SUPPORTED_OS[macos-x86_64]=darwin-x64
SUPPORTED_OS[ubuntu-x86_64]=linux-x64
SUPPORTED_OS[windows-x86_64]=win32-x64

TMP_DIR=$( mktemp -d -t vscode-create )
trap "clean_tmp_dir $TMP_DIR" EXIT

BIN_FILES=($BIN_DIR/*.tgz(.))

if (( ${#BIN_FILES[@]} != 4 )); then
    echo "Sui binary directory $BIN_DIR should only contain binaries for the four supported platforms"
    exit 1
fi


for SUI_ARCHIVE_PATH in "${BIN_FILES[@]}"; do
    # Extract just the file name
    FILE_NAME=${SUI_ARCHIVE_PATH##*/}
    # Remove the .tgz extension
    BASE_NAME=${FILE_NAME%.tgz}
    # Extract everything untl last `-`
    OS_NAME_PREFIX=${BASE_NAME%-*}
    # Extract everything after the last `-`
    OS_NAME=${OS_NAME_PREFIX##*-}
    # Extract everything after the last `-`
    OS_VARIANT=${BASE_NAME##*-}

    DIST_OS=$OS_NAME-$OS_VARIANT

    if [[ ! -v SUPPORTED_OS[$DIST_OS] ]]; then
        echo "Found Sui binary archive for a platform that is not supported:  $SUI_ARCHIVE_PATH"
        echo "Supported platforms:"
        for PLATFORM in ${(k)SUPPORTED_OS}; do
            echo "\t$PLATFORM"
        done
        exit 1
    fi

    rm -rf $TMP_DIR/$DIST_OS
    mkdir $TMP_DIR/$DIST_OS
    tar -xf $SUI_ARCHIVE_PATH --directory $TMP_DIR/$DIST_OS

    # name of the move-analyzer binary
    SERVER_BIN="move-analyzer"
    ARCHIVE_SERVER_BIN=$SERVER_BIN"-"$DIST_OS
    if [[ "$DIST_OS" == *"windows"* ]]; then
        SERVER_BIN="$SERVER_BIN".exe
    fi

    # copy move-analyzer binary to the appropriate location where it's picked up when bundling the
    # extension
    LANG_SERVER_DIR="language-server"
    rm -rf $LANG_SERVER_DIR
    mkdir $LANG_SERVER_DIR

    cp $TMP_DIR/$DIST_OS/$SERVER_BIN $LANG_SERVER_DIR

    VSCODE_OS=${SUPPORTED_OS[$DIST_OS]}
    vsce "$OP" ${OPTS//VSCODE_OS/$VSCODE_OS} --target "$VSCODE_OS"

    rm -rf $LANG_SERVER_DIR

done


# build a "generic" version of the extension that does not bundle the move-analyzer binary
vsce "$OP" ${OPTS//VSCODE_OS/generic}
