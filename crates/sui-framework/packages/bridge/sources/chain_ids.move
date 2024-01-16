// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::chain_ids {

    use std::vector;

    // Chain IDs
    const SuiMainnet: u8 = 0;
    const SuiTestnet: u8 = 1;
    const SuiDevnet: u8 = 2;

    const EthMainnet: u8 = 10;
    const EthSepolia: u8 = 11;

    const EInvalidBridgeRoute: u64 = 0;

    struct BridgeRoute has copy, drop, store {
        source: u8,
        destination: u8,
    }

    public fun sui_mainnet(): u8 {
        SuiMainnet
    }

    public fun sui_testnet(): u8 {
        SuiTestnet
    }

    public fun sui_devnet(): u8 {
        SuiDevnet
    }

    public fun eth_mainnet(): u8 {
        EthMainnet
    }

    public fun eth_sepolia(): u8 {
        EthSepolia
    }

    fun valid_routes(): vector<BridgeRoute> {
        vector[
            BridgeRoute { source: SuiMainnet, destination: EthMainnet },
            BridgeRoute { source: SuiDevnet, destination: EthSepolia },
            BridgeRoute { source: SuiTestnet, destination: EthSepolia },
            BridgeRoute { source: EthMainnet, destination: SuiMainnet },
            BridgeRoute { source: EthSepolia, destination: SuiDevnet },
            BridgeRoute { source: EthSepolia, destination: SuiTestnet }]
    }

    // Checks and return BridgeRoute if the route is supported by the bridge.
    public fun get_route(source: u8, destination: u8): BridgeRoute {
        let route = BridgeRoute { source, destination };
        return if (vector::contains(&valid_routes(), &route)) {
            route
        } else {
            abort EInvalidBridgeRoute
        }
    }
}
