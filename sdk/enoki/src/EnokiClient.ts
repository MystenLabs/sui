// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Proof } from './EnokiKeypair.js';

const DEFAULT_API_URL = 'https://api.enoki.mystenlabs.com';

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
	#apiKey: string;
	#apiUrl: string;

	constructor(config: EnokiClientConfig) {
		this.#apiKey = config.apiKey;
		this.#apiUrl = config.apiUrl ?? DEFAULT_API_URL;
	}

	getAuthProviders() {
		return this.#fetch<{
			authenticationProviders: {
				providerType: 'google' | 'facebook' | 'twitch';
				clientId: string;
				scopes: string[] | null;
			}[];
		}>('auth-providers', {}, { method: 'GET' });
	}

	getAddressForJWT(jwt: string, salt?: string) {
		return this.#fetch<{ address: string; salt: string }>(
			'address',
			{ jwt, salt },
			{ method: 'POST' },
		);
	}

	getSaltForJWT(jwt: string) {
		return this.#fetch<{ salt: string }>('salt', { jwt }, { method: 'POST' });
	}

	createProofForJWT(input: {
		jwt: string;
		extendedEphemeralPublicKey: string;
		maxEpoch: number;
		jwtRandomness: string;
		salt: string;
	}): Promise<Proof> {
		return this.#fetch('zkp', input, { method: 'POST' });
	}

	async #fetch<T = any>(
		path: string,
		payload: Record<string, unknown>,
		init: RequestInit,
	): Promise<T> {
		const filteredPayload = Object.fromEntries(
			Object.entries(payload).filter(([_, value]) => !!value),
		) as Record<string, string>;

		const searchParams = init.method === 'GET' ? new URLSearchParams(filteredPayload) : null;

		const res = await fetch(`${this.#apiUrl}/${path}${searchParams ? `?${searchParams}` : ''}`, {
			...init,
			headers: {
				...init.headers,
				Authorization: `Bearer ${this.#apiKey}`,
				'Content-Type': 'application/json',
				'Request-Id': crypto.randomUUID(),
			},
			body: init.method?.toLowerCase() === 'post' ? JSON.stringify(payload) : undefined,
		});

		if (!res.ok) {
			throw new Error('Failed to fetch');
		}

		const { data } = await res.json();

		return data as T;
	}
}
