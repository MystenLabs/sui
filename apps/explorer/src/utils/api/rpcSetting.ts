// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export enum Network {
    Local = 'Local',
    Static = 'Static',
    Devnet = 'Devnet',
    Staging = 'Staging',
    CI = 'CI',
}

const ENDPOINTS: Record<Network, string> = {
    [Network.Local]: 'http://127.0.0.1:9000',
    [Network.CI]: 'https://fullnode.devnet.sui.io:443',
    [Network.Devnet]: 'https://fullnode.devnet.sui.io:443',
    [Network.Staging]: 'https://fullnode.staging.sui.io:443',
    // NOTE: Static is pointed to devnet, but it shouldn't actually fetch data.
    [Network.Static]: 'https://fullnode.devnet.sui.io:443',
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
