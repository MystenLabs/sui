// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export enum Network {
    LOCAL = 'LOCAL',
    DEVNET = 'DEVNET',
    TESTNET = 'TESTNET',
}

const ENDPOINTS: Record<Network, string> = {
    [Network.LOCAL]: 'http://127.0.0.1:9000',
    [Network.DEVNET]: 'https://fullnode.devnet.sui.io:443',
    [Network.TESTNET]: 'https://fullnode-explorer.testnet.sui.io:443',
};

export function getEndpoint(network: Network | string): string {
    if (Object.keys(ENDPOINTS).includes(network)) {
        return ENDPOINTS[network as Network];
    }

    return network;
}
