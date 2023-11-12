// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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
	#webSocket: WebSocket | null;
	#subscriptions: Map<number, SubscriptionRequest & { id: number }>;
	#disconnects: number;

	constructor(endpoint: string, options: WebsocketClientOptions = {}) {
		this.endpoint = endpoint;

		this.options = { ...DEFAULT_CLIENT_OPTIONS, ...options };
		if (!this.options.WebSocketConstructor) {
			throw new Error('Missing WebSocket constructor');
		}

		if (this.endpoint.startsWith('http')) {
			this.endpoint = getWebsocketUrl(this.endpoint);
		}

		this.#webSocket = null;
		this.#subscriptions = new Map();
		this.#disconnects = 0;
	}

	#setupWebSocket() {
		if (this.#webSocket) {
			return this.#webSocket;
		}

		this.#webSocket = new WebSocket(this.endpoint);

		this.#webSocket.addEventListener('open', () => {
			this.#disconnects = 0;
		});

		this.#webSocket.addEventListener('close', () => {
			this.#disconnects++;
			if (this.#disconnects <= this.options.maxReconnects) {
				setTimeout(() => {
					this.#reconnect();
				}, this.options.reconnectTimeout);
			}
		});

		this.#webSocket.addEventListener('message', ({ data }) => {
			const params = data.params as NotificationMessageParams;

			this.#subscriptions.forEach((subscription) => {
				if (subscription.method === data.method && params.subscription === subscription.id) {
					subscription.onMessage(params.result);
				}
			});
		});

		return this.#webSocket;
	}

	#reconnect() {
		this.#webSocket?.close();
		this.#webSocket = null;

		this.#subscriptions.forEach((subscription) => this.request(subscription));
	}

	async request<T>(input: SubscriptionRequest<T>) {
		const webSocket = this.#setupWebSocket();
		// TODO: Need to wrap this up into a request / response model so that we actually can await this to get the ID:
		const id = webSocket.send(JSON.stringify({ method: input.method, params: input.params }));
		const initialId = input.initialId || id;
		this.#subscriptions.set(initialId, {
			...input,
			// Always set the latest actual subscription ID:
			id,
			initialId,
		});

		return async () => {
			const webSocket = this.#setupWebSocket();
			// NOTE: Due to reconnects, the inner subscription ID could have actually changed:
			const subscription = this.#subscriptions.get(initialId);
			if (!subscription) return false;

			this.#subscriptions.delete(initialId);

			return webSocket.send(
				JSON.stringify({ method: input.unsubscribe, params: [subscription.id] }),
				// TODO:
				// this.options.callTimeout,
			);
		};
	}
}
