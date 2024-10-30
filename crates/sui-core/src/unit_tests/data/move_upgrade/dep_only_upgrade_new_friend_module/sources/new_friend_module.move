// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module base_addr::new_friend_module;

public fun friend_call(): u64 { base_addr::base::friend_fun(1) }
