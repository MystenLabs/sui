// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JsonRpcClient, type RpcParams } from '@mysten/sui.js';
import * as Sentry from '@sentry/react';
import { type SpanStatusType } from '@sentry/tracing';

export class SentryRPCClient extends JsonRpcClient {
    #url: string;
    constructor(url: string) {
        super(url);
        this.#url = url;
    }

    async #withRequest(
        name: string,
        data: Record<string, unknown>,
        handler: () => Promise<unknown>
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

    async request(method: string, args: unknown[]) {
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
