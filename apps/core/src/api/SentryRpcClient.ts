// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JsonRpcClient } from '@mysten/sui.js';
import * as Sentry from '@sentry/react';

export class SentryRpcClient extends JsonRpcClient {
	#url: string;
	constructor(url: string) {
		super(url);
		this.#url = url;
	}

	async #withRequest(name: string, data: Record<string, unknown>, handler: () => Promise<unknown>) {
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
			const status: Sentry.SpanStatusType = 'ok';
			transaction.setStatus(status);
			return res;
		} catch (e) {
			const status: Sentry.SpanStatusType = 'internal_error';
			transaction.setStatus(status);
			throw e;
		} finally {
			transaction.finish();
		}
	}

	async request(method: string, args: any) {
		return this.#withRequest(method, { args }, () => super.request(method, args));
	}
}
