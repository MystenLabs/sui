// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JsonRpcError } from './errors.js';

function getWebsocketUrl(httpUrl: string): string {
	const url = new URL(httpUrl);
	url.protocol = url.protocol.replace('http', 'ws');
	return url.toString();
}

type JsonRpcMessage =
	| {
			id: number;
			result: never;
			error: {
				code: number;
				message: string;
			};
	  }
	| {
			id: number;
			result: unknown;
			error: never;
	  }
	| {
			method: string;
			params: NotificationMessageParams;
	  };

type NotificationMessageParams = {
	subscription?: number;
	result: object;
};

type SubscriptionRequest<T = any> = {
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
	 * Custom WebSocket class to use. Defaults to the global WebSocket class, if available.
	 */
	WebSocketConstructor?: typeof WebSocket;
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
	// We fudge the typing because we also check for undefined in the constructor:
	WebSocketConstructor: (typeof WebSocket !== 'undefined'
		? WebSocket
		: undefined) as typeof WebSocket,
	callTimeout: 30000,
	reconnectTimeout: 3000,
	maxReconnects: 5,
} satisfies WebsocketClientOptions;

export class WebsocketClient {
	endpoint: string;
	options: Required<WebsocketClientOptions>;
	#requestId = 0;
	#disconnects = 0;
	#webSocket: WebSocket | null = null;
	#connectionPromise: Promise<WebSocket> | null = null;
	#subscriptions = new Set<RpcSubscription>();
	#pendingRequests = new Map<
		number,
		{
			resolve: (result: Extract<JsonRpcMessage, { id: number }>) => void;
			reject: (reason: unknown) => void;
			timeout: ReturnType<typeof setTimeout>;
		}
	>();

	constructor(endpoint: string, options: WebsocketClientOptions = {}) {
		this.endpoint = endpoint;
		this.options = { ...DEFAULT_CLIENT_OPTIONS, ...options };

		if (!this.options.WebSocketConstructor) {
			throw new Error('Missing WebSocket constructor');
		}

		if (this.endpoint.startsWith('http')) {
			this.endpoint = getWebsocketUrl(this.endpoint);
		}
	}

	async makeRequest<T>(method: string, params: any[]): Promise<T> {
		const webSocket = await this.#setupWebSocket();

		return new Promise<Extract<JsonRpcMessage, { id: number }>>((resolve, reject) => {
			this.#requestId += 1;
			this.#pendingRequests.set(this.#requestId, {
				resolve: resolve,
				reject,
				timeout: setTimeout(() => {
					this.#pendingRequests.delete(this.#requestId);
					reject(new Error(`Request timeout: ${method}`));
				}, this.options.callTimeout),
			});

			webSocket.send(JSON.stringify({ jsonrpc: '2.0', id: this.#requestId, method, params }));
		}).then(({ error, result }) => {
			if (error) {
				throw new JsonRpcError(error.message, error.code);
			}

			return result as T;
		});
	}

	#setupWebSocket() {
		if (this.#connectionPromise) {
			return this.#connectionPromise;
		}

		this.#connectionPromise = new Promise<WebSocket>((resolve) => {
			this.#webSocket?.close();
			this.#webSocket = new this.options.WebSocketConstructor(this.endpoint);

			this.#webSocket.addEventListener('open', () => {
				this.#disconnects = 0;
				resolve(this.#webSocket!);
			});

			this.#webSocket.addEventListener('close', () => {
				this.#disconnects++;
				if (this.#disconnects <= this.options.maxReconnects) {
					setTimeout(() => {
						this.#reconnect();
					}, this.options.reconnectTimeout);
				}
			});

			this.#webSocket.addEventListener('message', ({ data }: { data: string }) => {
				let json: JsonRpcMessage;
				try {
					json = JSON.parse(data) as JsonRpcMessage;
				} catch (error) {
					console.error(new Error(`Failed to parse RPC message: ${data}`, { cause: error }));
					return;
				}

				if ('id' in json && json.id != null && this.#pendingRequests.has(json.id)) {
					const { resolve, timeout } = this.#pendingRequests.get(json.id)!;

					clearTimeout(timeout);
					resolve(json);
				} else if ('params' in json) {
					const { params } = json;
					this.#subscriptions.forEach((subscription) => {
						if (subscription.subscriptionId === params.subscription)
							if (params.subscription === subscription.subscriptionId) {
								subscription.onMessage(params.result);
							}
					});
				}
			});
		});

		return this.#connectionPromise;
	}

	async #reconnect() {
		this.#webSocket?.close();
		this.#connectionPromise = null;

		return Promise.allSettled(
			[...this.#subscriptions].map((subscription) => subscription.subscribe(this)),
		);
	}

	async subscribe<T>(input: SubscriptionRequest<T>) {
		const subscription = new RpcSubscription(input);
		this.#subscriptions.add(subscription);
		await subscription.subscribe(this);
		return () => subscription.unsubscribe(this);
	}
}

class RpcSubscription {
	subscriptionId: number | null = null;
	input: SubscriptionRequest<any>;
	subscribed = false;

	constructor(input: SubscriptionRequest) {
		this.input = input;
	}

	onMessage(message: unknown) {
		if (this.subscribed) {
			this.input.onMessage(message);
		}
	}

	async unsubscribe(client: WebsocketClient) {
		const { subscriptionId } = this;
		this.subscribed = false;
		if (subscriptionId == null) return false;
		this.subscriptionId = null;

		return client.makeRequest(this.input.unsubscribe, [subscriptionId]);
	}

	async subscribe(client: WebsocketClient) {
		this.subscriptionId = null;
		this.subscribed = true;
		const newSubscriptionId = await client.makeRequest<number>(
			this.input.method,
			this.input.params,
		);

		if (this.subscribed) {
			this.subscriptionId = newSubscriptionId;
		}
	}
}
