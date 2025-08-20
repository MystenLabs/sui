// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module addr::infinite_loop;

public fun inf_loop_0() { loop { continue } }
public fun inf_loop_1() { while (true) { continue } }
