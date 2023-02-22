// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    JsonRpcProvider,
    Connection,
    devnetConnection,
    localnetConnection,
} from '@mysten/sui.js';

export enum Network {
    LOCAL = 'LOCAL',
    DEVNET = 'DEVNET',
    TESTNET = 'TESTNET',
}

const CONNECTIONS: Record<Network, Connection> = {
    [Network.LOCAL]: localnetConnection,
    [Network.DEVNET]: devnetConnection,
    [Network.TESTNET]: new Connection({
        fullnode: 'https://fullnode-explorer.testnet.sui.io:443',
    }),
};

const defaultRpcMap: Map<Network | string, JsonRpcProvider> = new Map();
/** @deprecated This shouldn't be directly used, and instead should be used through `useRpc()`. */
export const DefaultRpcClient = (network: Network | string) => {
    const existingClient = defaultRpcMap.get(network);
    if (existingClient) return existingClient;

    const connection =
        network in Network
            ? CONNECTIONS[network as Network]
            : new Connection({ fullnode: network });

    const provider = new JsonRpcProvider(connection);
    defaultRpcMap.set(network, provider);
    return provider;
};
