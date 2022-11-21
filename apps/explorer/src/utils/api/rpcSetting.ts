// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export enum Network {
    LOCAL = 'LOCAL',
    STATIC = 'STATIC',
    DEVNET = 'DEVNET',
    STAGING = 'STAGING',
    TESTNET = 'TESTNET',
}

const ENDPOINTS: Record<Network, string> = {
    [Network.LOCAL]: 'http://127.0.0.1:9000',
    [Network.DEVNET]: 'https://fullnode.devnet.sui.io:443',
    [Network.STAGING]: 'https://fullnode.staging.sui.io:443',
    [Network.TESTNET]: 'https://fullnode.testnet.sui.io:443',

    // NOTE: Static is pointed to devnet, but it shouldn't actually fetch data.
    [Network.STATIC]: 'https://fullnode.devnet.sui.io:443',
};

export function getEndpoint(network: Network | string): string {
    // Endpoint has 2 types:
    // 1) Default URLs are to the Local RPC server, Staging, or DevNet
    if (Object.keys(ENDPOINTS).includes(network)) {
        return ENDPOINTS[network as Network];
    }

    // 2) Custom URL provided by the user
    return network;
}
