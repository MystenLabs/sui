// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	CreateSponsoredTransactionBlockApiInput,
	CreateSponsoredTransactionBlockApiResponse,
	CreateZkLoginNonceApiInput,
	CreateZkLoginNonceApiResponse,
	CreateZkLoginZkpApiInput,
	CreateZkLoginZkpApiResponse,
	ExecuteSponsoredTransactionBlockApiInput,
	ExecuteSponsoredTransactionBlockApiResponse,
	GetAppApiInput,
	GetAppApiResponse,
	GetZkLoginApiInput,
	GetZkLoginApiResponse,
} from './type.js';

const DEFAULT_API_URL = 'https://api.enoki.mystenlabs.com';
const ZKLOGIN_HEADER = 'zklogin-jwt';

export interface EnokiClientConfig {
	/** The API key for the Enoki app, available in the Enoki Portal. */
	apiKey: string;

	/** The API URL for Enoki. In most cases, this should not be set. */
	apiUrl?: string;
}

/**
 * A low-level client for interacting with the Enoki API.
 */
export class EnokiClient {
	#version: string;
	#apiUrl: string;
	#apiKey: string;

	constructor(config: EnokiClientConfig) {
		this.#version = 'v1';
		this.#apiUrl = config.apiUrl ?? DEFAULT_API_URL;
		this.#apiKey = config.apiKey;
	}

	getApp(_input?: GetAppApiInput) {
		return this.#fetch<GetAppApiResponse>('app', {
			method: 'GET',
		});
	}

	getZkLogin(input: GetZkLoginApiInput) {
		return this.#fetch<GetZkLoginApiResponse>('zklogin', {
			method: 'GET',
			headers: {
				[ZKLOGIN_HEADER]: input.jwt,
			},
		});
	}

	createZkLoginNonce(input: CreateZkLoginNonceApiInput) {
		return this.#fetch<CreateZkLoginNonceApiResponse>('zklogin/nonce', {
			method: 'POST',
			body: JSON.stringify({
				network: input.network,
				ephemeralPublicKey: input.ephemeralPublicKey.toSuiPublicKey(),
				additionalEpochs: input.additionalEpochs,
			}),
		});
	}

	createZkLoginZkp(input: CreateZkLoginZkpApiInput) {
		return this.#fetch<CreateZkLoginZkpApiResponse>('zklogin/zkp', {
			method: 'POST',
			headers: {
				[ZKLOGIN_HEADER]: input.jwt,
			},
			body: JSON.stringify({
				network: input.network,
				ephemeralPublicKey: input.ephemeralPublicKey.toSuiPublicKey(),
				maxEpoch: input.maxEpoch,
				randomness: input.randomness,
			}),
		});
	}

	createSponsoredTransactionBlock(input: CreateSponsoredTransactionBlockApiInput) {
		return this.#fetch<CreateSponsoredTransactionBlockApiResponse>('transaction-blocks/sponsor', {
			method: 'POST',
			headers: {
				[ZKLOGIN_HEADER]: input.jwt,
			},
			body: JSON.stringify({
				network: input.network,
				transactionBlockKindBytes: input.transactionBlockKindBytes,
			}),
		});
	}

	executeSponsoredTransactionBlock(input: ExecuteSponsoredTransactionBlockApiInput) {
		return this.#fetch<ExecuteSponsoredTransactionBlockApiResponse>(
			`transaction-blocks/sponsor/${input.digest}`,
			{
				method: 'POST',
				body: JSON.stringify({
					signature: input.signature,
				}),
			},
		);
	}

	async #fetch<T = unknown>(path: string, init: RequestInit): Promise<T> {
		const res = await fetch(`${this.#apiUrl}/${this.#version}/${path}`, {
			...init,
			headers: {
				...init.headers,
				Authorization: `Bearer ${this.#apiKey}`,
				'Content-Type': 'application/json',
				'Request-Id': crypto.randomUUID(),
			},
		});

		if (!res.ok) {
			throw new Error('Failed to fetch');
		}

		const { data } = await res.json();

		return data as T;
	}
}
