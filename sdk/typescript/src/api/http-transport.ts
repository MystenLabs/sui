// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { RequestManager, HTTPTransport, Client } from '@open-rpc/client-js';
import { PACKAGE_VERSION, TARGETED_RPC_VERSION } from '../version.js';
import type { WebsocketClientOptions } from '../rpc/websocket-client.js';
import { WebsocketClient } from '../rpc/websocket-client.js';

/**
 * An object defining headers to be passed to the RPC server
 */
export type HttpHeaders = { [header: string]: string };

interface SuiHTTPTransportOptions {
	network: SuiNetwork;
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
export type SuiNetwork =
	| 'mainnet'
	| 'testnet'
	| 'devnet'
	| 'localnet'
	| `http${'s' | ''}://${string}`;

const networkToEndpoint: Record<SuiNetwork, string> = {
	mainnet: 'https://fullnode.mainnet.sui.io:443',
	testnet: 'https://fullnode.testnet.sui.io:443',
	devnet: 'https://fullnode.devnet.sui.io:443',
	localnet: 'http://127.0.0.1:9000',
};

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
	private rpcClient: Client;
	private websocketClient: WebsocketClient;

	constructor({
		network,
		websocket: { url: websocketUrl, ...websocketOptions } = {} as WebsocketClientOptions,
		rpc,
	}: SuiHTTPTransportOptions) {
		const url = networkToEndpoint[network] ?? network;
		const transport = new HTTPTransport(rpc?.url ?? url, {
			headers: {
				'Content-Type': 'application/json',
				'Client-Sdk-Type': 'typescript',
				'Client-Sdk-Version': PACKAGE_VERSION,
				'Client-Target-Api-Version': TARGETED_RPC_VERSION,
				...rpc?.headers,
			},
		});

		this.rpcClient = new Client(new RequestManager([transport]));
		this.websocketClient = new WebsocketClient(websocketUrl ?? url, websocketOptions);
	}

	async request<T>(input: SuiTransportRequestOptions): Promise<T> {
		return await this.rpcClient.request(input);
	}

	async subscribe<T>(input: SuiTransportSubscribeOptions<T>): Promise<() => Promise<boolean>> {
		const unsubscribe = await this.websocketClient.request(input);

		return async () => !!(await unsubscribe());
	}
}
