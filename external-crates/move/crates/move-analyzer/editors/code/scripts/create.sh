#!/bin/zsh
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# This script is meant to be executed on MacOS (hence zsh use - to get associative arrays otherwise
# unavailable in the bundled bash version).

set -e

usage() {
    SCRIPT_NAME=$(basename "$1")
    >&2 echo "Usage: $SCRIPT_NAME -pkg|-pub [-h]"
    >&2 echo ""
    >&2 echo "Options:"
    >&2 echo " -pub          Publish extensions for all targets"
    >&2 echo " -pkg          Package extensions for all targets"
    >&2 echo " -h            Print this message"
}

clean_tmp_dir() {
  test -d "$TMP_DIR" && rm -fr "$TMP_DIR"
}

if [[ "$@" == "" ]]; then
    usage $0
    exit 1
fi

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
        usage $0
        exit 1
    fi
done

# these will have to change if we want a different network/version
NETWORK="testnet"
VERSION="1.13.0"

# a map from os version identifiers in Sui's binary distribution to os version identifiers
# representing VSCode's target platforms used for creating platform-specific plugin distributions
declare -A SUPPORTED_OS
SUPPORTED_OS[macos-arm64]=darwin-arm64
SUPPORTED_OS[macos-x86_64]=darwin-x64
SUPPORTED_OS[ubuntu-x86_64]=linux-x64
SUPPORTED_OS[windows-x86_64]=win32-x64

TMP_DIR=$( mktemp -d -t vscode-create )
trap "clean_tmp_dir $TMP_DIR" EXIT

for DIST_OS VSCODE_OS in "${(@kv)SUPPORTED_OS}"; do
    # Sui distribution identifier
    SUI_DISTRO=$NETWORK"-v"$VERSION
    # name of the Sui distribution archive file, for example sui-testnet-v1.13.0-macos-arm64.tgz
    SUI_ARCHIVE="sui-"$SUI_DISTRO"-"$DIST_OS".tgz"
    # a path to downloaded Sui archive
    SUI_ARCHIVE_PATH=$TMP_DIR"/"$SUI_ARCHIVE

    # download Sui archive file to a given location and uncompress it
    curl https://github.com/MystenLabs/sui/releases/download/"$SUI_DISTRO"/"$SUI_ARCHIVE" -L -o $SUI_ARCHIVE_PATH
    tar -xf $SUI_ARCHIVE_PATH --directory $TMP_DIR

    # names of the move-analyzer binary, both the one becoming part of the extension ($SERVER_BIN)
    # and the one in the Sui archive ($ARCHIVE_SERVER_BIN)
    SERVER_BIN="move-analyzer"
    ARCHIVE_SERVER_BIN=$SERVER_BIN"-"$DIST_OS
    if [[ "$DIST_OS" == *"windows"* ]]; then
        SERVER_BIN="$SERVER_BIN".exe
        ARCHIVE_SERVER_BIN="$ARCHIVE_SERVER_BIN".exe
    fi

    # copy move-analyzer binary to the appropriate location where it's picked up when bundling the
    # extension
    LANG_SERVER_DIR="language-server"
    rm -rf $LANG_SERVER_DIR
    mkdir $LANG_SERVER_DIR

    SRC_SERVER_BIN_LOC=$TMP_DIR"/external-crates/move/target/release/"$ARCHIVE_SERVER_BIN
    DST_SERVER_BIN_LOC=$LANG_SERVER_DIR"/"$SERVER_BIN
    cp $SRC_SERVER_BIN_LOC $DST_SERVER_BIN_LOC

    vsce "$OP" ${OPTS//VSCODE_OS/$VSCODE_OS} --target "$VSCODE_OS"

    rm -rf $LANG_SERVER_DIR

done


# build a "generic" version of the extension that does not bundle the move-analyzer binary
vsce "$OP" ${OPTS//VSCODE_OS/generic}
