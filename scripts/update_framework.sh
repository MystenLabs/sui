#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0



pushd ./crates/sui-framework
UPDATE=1 cargo test build_system_packages
popd

pushd ./crates/sui-framework-snapshot
cargo run --bin sui-framework-snapshot
popd
