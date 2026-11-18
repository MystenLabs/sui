// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that run without the simulator. The test job builds only this binary, because the
// other test binaries compile to nothing outside msim.
#![cfg(not(msim))]

mod db_shell_tests;
mod protocol_version_tests;
mod traffic_control_tests;
