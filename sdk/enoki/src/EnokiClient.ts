// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { PublicKey } from '@mysten/sui.js/cryptography';
import type { ZkLoginSignatureInputs } from '@mysten/sui.js/zklogin';

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
	#version: string;
	#apiUrl: string;
	#apiKey: string;

	constructor(config: EnokiClientConfig) {
		this.#version = 'v1';
		this.#apiUrl = config.apiUrl ?? DEFAULT_API_URL;
		this.#apiKey = config.apiKey;
	}

	getAuthProviders() {
		return this.#fetch<{
			authenticationProviders: {
				providerType: 'google' | 'facebook' | 'twitch';
				clientId: string;
			}[];
		}>('config/auth-providers', {
			method: 'GET',
		});
	}

	getZkLogin(input: { jwt: string }) {
		return this.#fetch<{ address: string; salt: string }>('zklogin', {
			method: 'GET',
			headers: {
				'zk-login-jwt': input.jwt,
			},
		});
	}

	createNonce(input: { ephemeralPublicKey: PublicKey }) {
		return this.#fetch<{
			nonce: string;
			randomness: string;
			epoch: number;
			maxEpoch: number;
			estimatedExpiration: number;
		}>('zklogin/nonce', {
			method: 'POST',
			body: JSON.stringify({
				ephemeralPublicKey: input.ephemeralPublicKey.toSuiPublicKey(),
			}),
		});
	}

	createZkLoginZkp(input: {
		jwt: string;
		ephemeralPublicKey: PublicKey;
		randomness: string;
		maxEpoch: number;
	}) {
		return this.#fetch<ZkLoginSignatureInputs>('zklogin/kzp', {
			method: 'POST',
			headers: {
				'zk-login-jwt': input.jwt,
			},
			body: JSON.stringify({
				ephemeralPublicKey: input.ephemeralPublicKey.toSuiPublicKey(),
				maxEpoch: input.maxEpoch,
				randomness: input.randomness,
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
			throw new Error('Failed to fetch');
		}

		const { data } = await res.json();

		return data as T;
	}
}
