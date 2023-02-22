// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    JsonRpcProvider,
    Connection,
    devnetConnection,
    localnetConnection,
    JsonRpcClient,
    type RpcParams,
} from '@mysten/sui.js';
import * as Sentry from '@sentry/react';
import { type SpanStatusType } from '@sentry/tracing';

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

class SentryRPCClient extends JsonRpcClient {
    #url: string;
    constructor(url: string) {
        super(url);
        this.#url = url;
    }

    async #withRequest(
        name: string,
        data: Record<string, any>,
        handler: () => Promise<any>
    ) {
        const transaction = Sentry.startTransaction({
            name,
            op: 'http.rpc-request',
            data: data,
            tags: {
                url: this.#url,
            },
        });

        try {
            const res = await handler();
            const status: SpanStatusType = 'ok';
            transaction.setStatus(status);
            return res;
        } catch (e) {
            const status: SpanStatusType = 'internal_error';
            transaction.setStatus(status);
            throw e;
        } finally {
            transaction.finish();
        }
    }

    async request(method: string, args: any[]) {
        return this.#withRequest(method, { args }, () =>
            super.request(method, args)
        );
    }

    async batchRequest(requests: RpcParams[]) {
        return this.#withRequest('batch', { requests }, () =>
            super.batchRequest(requests)
        );
    }
}

const defaultRpcMap: Map<Network | string, JsonRpcProvider> = new Map();
/** @deprecated This shouldn't be directly used, and instead should be used through `useRpc()`. */
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
                ? new SentryRPCClient(connection.fullnode)
                : undefined,
    });
    defaultRpcMap.set(network, provider);
    return provider;
};
