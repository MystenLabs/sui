// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Client } from '@hey-api/client-fetch';
import { createClient } from '@hey-api/client-fetch';

import type {
	PostV1TransactionBlocksSponsorByDigestData,
	PostV1TransactionBlocksSponsorData,
	PostV1ZkloginNonceData,
	PostV1ZkloginZkpData,
} from './generated-client/index.js';
import {
	getV1App,
	getV1Zklogin,
	postV1TransactionBlocksSponsor,
	postV1TransactionBlocksSponsorByDigest,
	postV1ZkloginNonce,
	postV1ZkloginZkp,
} from './generated-client/index.js';

const DEFAULT_API_URL = 'https://api.enoki.mystenlabs.com';

export interface EnokiClientConfig {
	/** The API key for the Enoki app, available in the Enoki Portal. */
	apiKey: string;

	/** The API URL for Enoki. In most cases, this should not be set. */
	apiUrl?: string;
}

export class EnokiClientError extends Error {
	errors: { code: string; message: string; data: unknown }[] = [];

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
	}
}

type RequestBody<T extends { body?: unknown }> = T extends { body?: infer U }
	? Exclude<U, undefined>
	: never;

/**
 * A low-level client for interacting with the Enoki API.
 */
export class EnokiClient {
	#client: Client;

	constructor(config: EnokiClientConfig) {
		this.#client = createClient({
			baseUrl: config.apiUrl ?? DEFAULT_API_URL,
			headers: {
				Authorization: `Bearer ${config.apiKey}`,
			},
		});
	}

	getApp() {
		return getV1App({
			client: this.#client,
		});
	}

	getZkLogin(input: { jwt: string }) {
		return getV1Zklogin({
			client: this.#client,
			headers: {
				'zklogin-jwt': input.jwt,
			},
		});
	}

	createZkLoginNonce(input: RequestBody<PostV1ZkloginNonceData>) {
		return postV1ZkloginNonce({
			client: this.#client,
			body: input,
		});
	}

	// ephemeralPublicKey: input.ephemeralPublicKey.toSuiPublicKey(),
	createZkLoginZkp(input: RequestBody<PostV1ZkloginZkpData> & { jwt: string }) {
		return postV1ZkloginZkp({
			client: this.#client,
			body: input,
			headers: {
				'zklogin-jwt': input.jwt,
			},
		});
	}

	createSponsoredTransaction(
		input: RequestBody<PostV1TransactionBlocksSponsorData> & { jwt?: string },
	) {
		return postV1TransactionBlocksSponsor({
			client: this.#client,
			body: input,
			headers: input.jwt
				? {
						'zklogin-jwt': input.jwt,
					}
				: {},
		});
	}

	executeSponsoredTransaction({
		digest,
		...body
	}: PostV1TransactionBlocksSponsorByDigestData['path'] &
		RequestBody<PostV1TransactionBlocksSponsorByDigestData>) {
		return postV1TransactionBlocksSponsorByDigest({
			client: this.#client,
			path: { digest },
			body,
		});
	}
}
