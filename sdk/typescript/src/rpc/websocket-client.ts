// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Client, RequestManager, WebSocketTransport } from '@open-rpc/client-js';

export const getWebsocketUrl = (httpUrl: string, port?: number): string => {
	const url = new URL(httpUrl);
	url.protocol = url.protocol.replace('http', 'ws');
	if (port) {
		url.port = port.toString();
	}
	return url.toString();
};

type NotificationMessageParams = {
	subscription: number;
	result: object;
};

type SubscriptionRequest<T = any> = {
	id?: number;
	initialId?: number;
	method: string;
	unsubscribe: string;
	params: any[];
	onMessage: (event: T) => void;
};

/**
 * Configuration options for the websocket connection
 */
export type WebsocketClientOptions = {
	/**
	 * Milliseconds before timing out while calling an RPC method
	 */
	callTimeout?: number;
	/**
	 * Milliseconds between attempts to connect
	 */
	reconnectTimeout?: number;
	/**
	 * Maximum number of times to try connecting before giving up
	 */
	maxReconnects?: number;
};

export const DEFAULT_CLIENT_OPTIONS = {
	callTimeout: 30000,
	reconnectTimeout: 3000,
	maxReconnects: 5,
} satisfies WebsocketClientOptions;

export class WebsocketClient {
	endpoint: string;
	options: Required<WebsocketClientOptions>;
	#client: Client | null;
	#subscriptions: Map<number, SubscriptionRequest & { id: number }>;
	#disconnects: number;

	constructor(endpoint: string, options: WebsocketClientOptions = {}) {
		this.endpoint = endpoint;
		this.options = { ...DEFAULT_CLIENT_OPTIONS, ...options };

		if (this.endpoint.startsWith('http')) {
			this.endpoint = getWebsocketUrl(this.endpoint);
		}

		this.#client = null;
		this.#subscriptions = new Map();
		this.#disconnects = 0;
	}

	#setupClient() {
		if (this.#client) {
			return this.#client;
		}

		const transport = new WebSocketTransport(this.endpoint);
		const requestManager = new RequestManager([transport]);
		this.#client = new Client(requestManager);

		transport.connection.addEventListener('open', () => {
			this.#disconnects = 0;
		});

		transport.connection.addEventListener('close', () => {
			this.#disconnects++;
			if (this.#disconnects <= this.options.maxReconnects) {
				setTimeout(() => {
					this.#reconnect();
				}, this.options.reconnectTimeout);
			}
		});

		this.#client.onNotification((data) => {
			const params = data.params as NotificationMessageParams;

			this.#subscriptions.forEach((subscription) => {
				if (subscription.method === data.method && params.subscription === subscription.id) {
					subscription.onMessage(params.result);
				}
			});
		});

		return this.#client;
	}

	#reconnect() {
		this.#client?.close();
		this.#client = null;

		this.#subscriptions.forEach((subscription) => this.request(subscription));
	}

	async request<T>(input: SubscriptionRequest<T>) {
		const client = this.#setupClient();
		const id = await client.request(
			{ method: input.method, params: input.params },
			this.options.callTimeout,
		);
		const initialId = input.initialId || id;
		this.#subscriptions.set(initialId, {
			...input,
			// Always set the latest actual subscription ID:
			id,
			initialId,
		});

		return async () => {
			const client = this.#setupClient();
			// NOTE: Due to reconnects, the inner subscription ID could have actually changed:
			const subscription = this.#subscriptions.get(initialId);
			if (!subscription) return false;

			this.#subscriptions.delete(initialId);

			return client.request(
				{ method: input.unsubscribe, params: [subscription.id] },
				this.options.callTimeout,
			);
		};
	}
}
