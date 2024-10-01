// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module root::main;

use local::main as local_main;
use local_without_lockfile::main as local_without_lockfile_main;
use onchain::main as onchain_main;

public fun main() {
    local_main::dummy();
    local_without_lockfile_main::dummy();
    onchain_main::dummy();
}
