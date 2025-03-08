// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::example;

use bridge::bridge;

public fun bridge_update_node_url(bridge: &mut bridge::Bridge, new_url: vector<u8>, ctx: &TxContext) {
  bridge::update_node_url(bridge, new_url, ctx)
}
