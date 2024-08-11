// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiHTTPTransport } from '@mysten/sui/client';
import * as Sentry from '@sentry/react';

const IGNORED_METHODS = ['suix_resolveNameServiceNames', 'suix_resolveNameServiceAddresses'];

export class SentryHttpTransport extends SuiHTTPTransport {
	#url: string;
	constructor(url: string) {
		super({ url });
		this.#url = url;
	}

	async #withRequest<T>(input: { method: string; params: unknown[] }, handler: () => Promise<T>) {
		const transaction = Sentry.startTransaction({
			name: input.method,
			op: 'http.rpc-request',
			data: input.params,
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

	override async request<T>(input: { method: string; params: unknown[] }) {
		if (IGNORED_METHODS.includes(input.method)) {
			return super.request<T>(input);
		}

		return this.#withRequest(input, () => super.request<T>(input));
	}
}
