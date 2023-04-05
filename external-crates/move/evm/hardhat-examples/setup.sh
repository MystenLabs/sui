#!/bin/bash
# Copyright (c) The Diem Core Contributors
# Copyright (c) The Move Contributors
# SPDX-License-Identifier: Apache-2.0

# Hardhat packages, along with their dependencies, are not checked into the Move repo.
# Use this script to get them downloaded and configured.

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

cd $SCRIPT_DIR

npm install --save-dev hardhat
