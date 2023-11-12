// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WebsocketClientOptions } from '../rpc/websocket-client.js';
import { WebsocketClient } from '../rpc/websocket-client.js';
import { PACKAGE_VERSION, TARGETED_RPC_VERSION } from '../version.js';

/**
 * An object defining headers to be passed to the RPC server
 */
export type HttpHeaders = { [header: string]: string };

interface SuiHTTPTransportOptions {
	url: string;
	rpc?: {
		headers?: HttpHeaders;
		url?: string;
	};
	websocket?: WebsocketClientOptions & {
		url?: string;
	};
}

export interface SuiTransportRequestOptions {
	method: string;
	params: unknown[];
}

// eslint-disable-next-line @typescript-eslint/ban-types

export interface SuiTransportSubscribeOptions<T> {
	method: string;
	unsubscribe: string;
	params: unknown[];
	onMessage: (event: T) => void;
}

export interface SuiTransport {
	request<T = unknown>(input: SuiTransportRequestOptions): Promise<T>;
	subscribe<T = unknown>(input: SuiTransportSubscribeOptions<T>): Promise<() => Promise<boolean>>;
}

export class SuiHTTPTransport implements SuiTransport {
	#requestId = 0;
	#options: SuiHTTPTransportOptions;
	#websocketClient?: WebsocketClient;

	constructor(options: SuiHTTPTransportOptions) {
		this.#options = options;
	}

	#getWebsocketClient(): WebsocketClient {
		if (!this.#websocketClient) {
			this.#websocketClient = new WebsocketClient(
				this.#options.websocket?.url ?? this.#options.url,
				this.#options.websocket,
			);
		}

		return this.#websocketClient;
	}

	async request<T>(input: SuiTransportRequestOptions): Promise<T> {
		this.#requestId += 1;

		const res = await fetch(this.#options.rpc?.url ?? this.#options.url, {
			headers: {
				'Content-Type': 'application/json',
				'Client-Sdk-Type': 'typescript',
				'Client-Sdk-Version': PACKAGE_VERSION,
				'Client-Target-Api-Version': TARGETED_RPC_VERSION,
				...this.#options.rpc?.headers,
			},
			body: JSON.stringify({
				jsonrpc: '2.0',
				id: this.#requestId,
				method: input.method,
				params: input.params,
			}),
		});

		if (!res.ok) {
			throw new Error('TODO: Real error:');
		}

		const data = await res.json();

		return data.result;
	}

	async subscribe<T>(input: SuiTransportSubscribeOptions<T>): Promise<() => Promise<boolean>> {
		const unsubscribe = await this.#getWebsocketClient().request(input);

		return async () => !!(await unsubscribe());
	}
}
