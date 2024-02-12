// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::chain_ids {

    use std::vector;

    // Chain IDs
    const SuiMainnet: u8 = 0;
    const SuiTestnet: u8 = 1;
    const SuiDevnet: u8 = 2;
    const SuiLocalTest: u8 = 3;

    const EthMainnet: u8 = 10;
    const EthSepolia: u8 = 11;
    const EthLocalTest: u8 = 12;

    struct BridgeRoute has drop {
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

    public fun sui_local_test(): u8 {
        SuiLocalTest
    }

    public fun eth_mainnet(): u8 {
        EthMainnet
    }

    public fun eth_sepolia(): u8 {
        EthSepolia
    }

    public fun eth_local_test(): u8 {
        EthLocalTest
    }

    public fun valid_routes(): vector<BridgeRoute> {
        vector[
            BridgeRoute { source: SuiMainnet, destination: EthMainnet },
            BridgeRoute { source: SuiDevnet, destination: EthSepolia },
            BridgeRoute { source: SuiTestnet, destination: EthSepolia },
            BridgeRoute { source: SuiLocalTest, destination: EthLocalTest },
            BridgeRoute { source: EthMainnet, destination: SuiMainnet },
            BridgeRoute { source: EthSepolia, destination: SuiDevnet },
            BridgeRoute { source: EthSepolia, destination: SuiTestnet },
            BridgeRoute { source: EthLocalTest, destination: SuiLocalTest }]
    }

    public fun is_valid_route(source: u8, destination: u8): bool {
        let route = BridgeRoute { source, destination };
        return vector::contains(&valid_routes(), &route)
    }
}
