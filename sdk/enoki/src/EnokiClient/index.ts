// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	CreateSponsoredTransactionApiInput,
	CreateSponsoredTransactionApiResponse,
	CreateSubnameApiInput,
	CreateSubnameApiResponse,
	CreateZkLoginNonceApiInput,
	CreateZkLoginNonceApiResponse,
	CreateZkLoginZkpApiInput,
	CreateZkLoginZkpApiResponse,
	DeleteSubnameApiInput,
	DeleteSubnameApiResponse,
	ExecuteSponsoredTransactionApiInput,
	ExecuteSponsoredTransactionApiResponse,
	GetAppApiInput,
	GetAppApiResponse,
	GetSubnamesApiInput,
	GetSubnamesApiResponse,
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

export class EnokiClientError extends Error {
	errors: { code: string; message: string; data: unknown }[] = [];
	status: number;
	code: string;

	constructor(status: number, response: string) {
		let errors;
		try {
			const parsedResponse = JSON.parse(response) as {
				errors: { code: string; message: string; data: unknown }[];
			};
			errors = parsedResponse.errors;
		} catch (e) {
			// Ignore
		}
		const cause = errors?.[0] ? new Error(errors[0].message) : undefined;
		super(`Request to Enoki API failed (status: ${status})`, {
			cause,
		});
		this.errors = errors ?? [];
		this.name = 'EnokiClientError';
		this.status = status;
		this.code = errors?.[0]?.code ?? 'unknown_error';
	}
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

	createSponsoredTransaction(input: CreateSponsoredTransactionApiInput) {
		return this.#fetch<CreateSponsoredTransactionApiResponse>('transaction-blocks/sponsor', {
			method: 'POST',
			headers: input.jwt
				? {
						[ZKLOGIN_HEADER]: input.jwt,
					}
				: {},
			body: JSON.stringify({
				sender: input.sender,
				network: input.network,
				transactionBlockKindBytes: input.transactionKindBytes,
				allowedAddresses: input.allowedAddresses,
				allowedMoveCallTargets: input.allowedMoveCallTargets,
			}),
		});
	}

	executeSponsoredTransaction(input: ExecuteSponsoredTransactionApiInput) {
		return this.#fetch<ExecuteSponsoredTransactionApiResponse>(
			`transaction-blocks/sponsor/${input.digest}`,
			{
				method: 'POST',
				body: JSON.stringify({
					signature: input.signature,
				}),
			},
		);
	}

	getSubnames(input: GetSubnamesApiInput) {
		const query = new URLSearchParams();
		if (input.address) {
			query.set('address', input.address);
		}
		if (input.network) {
			query.set('network', input.network);
		}
		if (input.domain) {
			query.set('domain', input.domain);
		}
		return this.#fetch<GetSubnamesApiResponse>(
			'subnames' + (query.size > 0 ? `?${query.toString()}` : ''),
			{
				method: 'GET',
			},
		);
	}

	createSubname(input: CreateSubnameApiInput) {
		return this.#fetch<CreateSubnameApiResponse>('subnames', {
			method: 'POST',
			headers: input.jwt
				? {
						[ZKLOGIN_HEADER]: input.jwt,
					}
				: {},
			body: JSON.stringify({
				network: input.network,
				domain: input.domain,
				subname: input.subname,
				targetAddress: input.targetAddress,
			}),
		});
	}

	deleteSubname(input: DeleteSubnameApiInput) {
		this.#fetch<DeleteSubnameApiResponse>('subnames', {
			method: 'DELETE',
			body: JSON.stringify({
				network: input.network,
				domain: input.domain,
				subname: input.subname,
			}),
		});
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
			throw new EnokiClientError(res.status, await res.text());
		}

		const { data } = await res.json();

		return data as T;
	}
}
