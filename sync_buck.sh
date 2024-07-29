#!/usr/bin/env sh
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

ensure_buck_fs() {
cat <<EOF > .buckconfig
[repositories]
root = .
prelude = ./buck/prelude
prelude-mysten = ./buck/prelude-mysten
toolchains = ./buck/toolchains
none = none

[repository_aliases]
config = prelude
ovr_config = prelude
fbcode = none
fbsource = none
fbcode_macros = none
buck = none

[parser]
target_platform_detector_spec = target:root//...->prelude//platforms:default

[project]
ignore = .git
EOF
touch .buckroot
}

clone() {
# Variables
OPERATION=$1
REPO_URL=$2
TEMP_DIR=$(mktemp -d)
TARGET_DIR="buck/"

# Ensure the target directory exists, create it if it doesn't
if [ ! -d "$TARGET_DIR" ]; then
    mkdir -p $TARGET_DIR
    if [ $? -eq 0 ]; then
        echo "Target directory $TARGET_DIR created"
    else
        echo "Failed to create target directory $TARGET_DIR"
        exit 1
    fi
fi
echo "Running $1"

# Shallow clone the repository into the temporary directory
git clone --depth 1 $REPO_URL $TEMP_DIR

# Check if the cloning was successful
if [ $? -eq 0 ]; then
    echo "Repository cloned successfully into $TEMP_DIR"
else
    echo "Failed to clone repository"
    exit 1
fi

echo "Copying files..."
# Copy the contents to the target directory
cp -r $TEMP_DIR/* $TARGET_DIR

# Check if the copy was successful
if [ $? -eq 0 ]; then
    echo "Files copied successfully to $TARGET_DIR"
else
    echo "Failed to copy files"
    exit 1
fi

# Clean up: remove the temporary directory
rm -rf $TEMP_DIR

echo "Temporary directory $TEMP_DIR removed"

}

main() {
    ensure_buck_fs
    clone "clone prelude" "git@github.com:suiwombat/buck_prelude.git"
    clone "clone vendor deps" "git@github.com:suiwombat/buck_sui.git"
}

main