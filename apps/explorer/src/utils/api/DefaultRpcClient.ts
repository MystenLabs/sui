// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SentryRpcClient } from '@mysten/core';
import {
    JsonRpcProvider,
    Connection,
    localnetConnection,
} from '@mysten/sui.js';

export enum Network {
    LOCAL = 'LOCAL',
    DEVNET = 'DEVNET',
    TESTNET = 'TESTNET',
    MAINNET = 'MAINNET',
}

const CONNECTIONS: Record<Network, Connection> = {
    [Network.LOCAL]: localnetConnection,
    [Network.DEVNET]: new Connection({
        fullnode: 'https://explorer-rpc.devnet.sui.io:443',
    }),
    [Network.TESTNET]: new Connection({
        fullnode: 'https://explorer-rpc.testnet.sui.io:443',
    }),
    [Network.MAINNET]: new Connection({
        fullnode: 'https://explorer-rpc.mainnet.sui.io:443',
    }),
};

const defaultRpcMap: Map<Network | string, JsonRpcProvider> = new Map();

// NOTE: This class should not be used directly in React components, prefer to use the useRpcClient() hook instead
export const DefaultRpcClient = (network: Network | string) => {
    const existingClient = defaultRpcMap.get(network);
    if (existingClient) return existingClient;

    const connection =
        network in Network
            ? CONNECTIONS[network as Network]
            : new Connection({ fullnode: network });

    const provider = new JsonRpcProvider(connection, {
        rpcClient:
            // If the network is a known network, and not localnet, attach the sentry RPC client for instrumentation:
            network in Network && network !== Network.LOCAL
                ? new SentryRpcClient(connection.fullnode)
                : undefined,
    });
    defaultRpcMap.set(network, provider);
    return provider;
};
