// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiClientGraphQLTransport } from '@mysten/graphql-transport';
import { SuiHTTPTransport } from '@mysten/sui.js/client';
import * as Sentry from '@sentry/react';

const IGNORED_METHODS = ['suix_resolveNameServiceNames', 'suix_resolveNameServiceAddresses'];

interface SentryHttpTransportOptions {
	url: string;
	graphqlUrl?: string;
	mode?: 'http' | 'graphql';
}

export class SentryHttpTransport extends SuiHTTPTransport {
	#url: string;
	#graphqlUrl?: string;
	#mode: 'http' | 'graphql';
	#graphqlTransport?: SuiClientGraphQLTransport;

	constructor(options: SentryHttpTransportOptions) {
		super({ url: options.url });
		this.#mode = options.mode || 'http';
		this.#url = options.url;
		this.#graphqlUrl = options.graphqlUrl;

		if (this.#mode === 'graphql') {
			if (!this.#graphqlUrl) {
				throw new Error('GraphQL URL is required for GraphQL mode');
			}

			this.#graphqlTransport = new SuiClientGraphQLTransport({
				url: this.#graphqlUrl,
				fallbackFullNodeUrl: this.#url,
			});
		}
	}

	async #withRequest<T>(
		url: string,
		input: { method: string; params: unknown[] },
		handler: () => Promise<T>,
	) {
		const transaction = Sentry.startTransaction({
			name: input.method,
			op: this.#mode === 'graphql' ? 'graphql.rpc-request' : 'http.rpc-request',
			data: input.params,
			tags: {
				url,
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

		return this.#withRequest(this.#mode === 'graphql' ? this.#graphqlUrl! : this.#url, input, () =>
			this.#mode === 'graphql'
				? this.#graphqlTransport!.request<T>(input)
				: super.request<T>(input),
		);
	}
}
