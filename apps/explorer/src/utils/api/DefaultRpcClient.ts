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

    async request(method: string, args: any[]) {
        const transaction = Sentry.startTransaction({
            name: method,
            op: 'http.rpc-request',
            data: { url: this.#url, args },
        });

        try {
            const res = await super.request(method, args);
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

    async batchRequest(requests: RpcParams[]) {
        const transaction = Sentry.startTransaction({
            name: 'batch',
            op: 'http.rpc-request',
            data: { url: this.#url, requests },
        });

        try {
            const res = await super.batchRequest(requests);
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
            // If the network is a known network, then attach the sentry RPC client for instrumentation:
            network in Network
                ? new SentryRPCClient(connection.fullnode)
                : undefined,
    });
    defaultRpcMap.set(network, provider);
    return provider;
};
