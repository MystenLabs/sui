// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import { fromB64 } from '@mysten/sui.js/utils';
import { ZkLoginSignatureInputs } from '@mysten/sui.js/zklogin';
import { decodeJwt } from 'jose';
import type { WritableAtom } from 'nanostores';
import { allTasks, atom, onMount, onSet, task } from 'nanostores';

import type { Encryption } from './encryption.js';
import { createDefaultEncryption } from './encryption.js';
import type { EnokiClientConfig } from './EnokiClient.js';
import { EnokiClient } from './EnokiClient.js';
import { EnokiKeypair } from './EnokiKeypair.js';
import { validateJWT } from './jwt.js';
import type { AsyncStore } from './stores.js';
import { createSessionStorage } from './stores.js';

export interface EnokiFlowConfig extends EnokiClientConfig {
	/**
	 * The storage interface to persist Enoki data locally.
	 * If not provided, it will use a sessionStorage-backed store.
	 */
	store?: AsyncStore;
	/**
	 * The key that will be used to store Enoki data locally.
	 * This will be passed to the configured `store` interface.
	 */
	storeKey?: string;
	/**
	 * The encryption interface that will be used to encrypt data before storing it locally.
	 * If not provided, it will use a default encryption interface.
	 */
	encryption?: Encryption;
	/**
	 * The encryption key that will be used to encrypt data before storing it locally.
	 * If not provided, it will use your Enoki API key as an encryption key.
	 */
	encryptionKey?: string;
}

interface EnokiFlowState {
	provider?: AuthProvider;
	address?: string;
	salt?: string;
	// Expiring data related to the proof:
	zkp?: {
		ephemeralKeyPair: string;
		maxEpoch: number;
		randomness: string;
		expiresAt: number;

		jwt?: string;
		proof?: ZkLoginSignatureInputs;
	};
}

export type AuthProvider = 'google' | 'facebook' | 'twitch';

const DEFAULT_STORAGE_KEY = '@enoki/flow';

export class EnokiFlow {
	#enokiClient: EnokiClient;
	#encryption: Encryption;
	#encryptionKey: string;
	#store: AsyncStore;
	#storeKey: string;

	$initialized: WritableAtom<boolean>;
	$state: WritableAtom<EnokiFlowState>;

	constructor(config: EnokiFlowConfig) {
		this.$state = atom({});
		this.$initialized = atom(false);

		this.#enokiClient = new EnokiClient({
			apiKey: config.apiKey,
			apiUrl: config.apiUrl,
		});
		this.#encryption = config.encryption ?? createDefaultEncryption();
		this.#encryptionKey = config.encryptionKey ?? config.apiKey;
		this.#store = config.store ?? createSessionStorage();
		this.#storeKey = config.storeKey ?? DEFAULT_STORAGE_KEY;

		onSet(this.$state, ({ newValue }) => {
			task(async () => {
				const storedValue = await this.#encryption.encrypt(
					this.#encryptionKey,
					JSON.stringify(newValue),
				);

				await this.#store.set(this.#storeKey, storedValue);
			});
		});

		onMount(this.$state, () => {
			this.restore();
		});
	}

	get enokiClient() {
		return this.#enokiClient;
	}

	// TODO: Probably name this better:
	// Maybe something like `createAuthorizationURL`?
	async startFlow(input: {
		provider: AuthProvider;
		clientId: string;
		redirectUrl: string;
		extraParams?: Record<string, unknown>;
	}) {
		const ephemeralKeyPair = new Ed25519Keypair();
		const { nonce, randomness, maxEpoch, estimatedExpiration } =
			await this.#enokiClient.createNonce({
				ephemeralPublicKey: ephemeralKeyPair.getPublicKey(),
			});

		const params = new URLSearchParams({
			...input.extraParams,
			nonce,
			client_id: input.clientId,
			redirect_uri: input.redirectUrl,
			response_type: 'id_token',
			// TODO: Eventually fetch the scopes for this client ID from the Enoki service:
			scope: [
				'openid',
				// Merge the requested scopes in with the required openid scopes:
				...(input.extraParams && 'scope' in input.extraParams
					? (input.extraParams.scope as string[])
					: []),
			]
				.filter(Boolean)
				.join(' '),
		});

		let oauthUrl: string;
		switch (input.provider) {
			case 'google': {
				oauthUrl = `https://accounts.google.com/o/oauth2/v2/auth?${params}`;
				break;
			}

			case 'facebook': {
				oauthUrl = `https://www.facebook.com/v17.0/dialog/oauth?${params}`;
				break;
			}

			case 'twitch': {
				params.set('force_verify', 'true');
				oauthUrl = `https://id.twitch.tv/oauth2/authorize?${params}`;
				break;
			}

			default:
				throw new Error(`Invalid provider: ${input.provider}`);
		}

		this.$state.set({
			provider: input.provider,
			zkp: {
				expiresAt: estimatedExpiration,
				maxEpoch,
				randomness,
				ephemeralKeyPair: ephemeralKeyPair.export().privateKey,
			},
		});

		// Allow the state to persist into stores before we redirect:
		await allTasks();

		return oauthUrl;
	}

	// TODO: Should our SDK manage this automatically in addition to exposing a method?
	// TODO: Should we rename this? Something with "callback" maybe?
	async handleAuthRedirect(hash: string = window.location.hash) {
		const params = new URLSearchParams(hash.startsWith('#') ? hash.slice(1) : hash);

		// Before we handle the auth redirect and get the state, we need to restore it:
		await this.restore();

		const state = this.$state.get();
		if (!state.zkp || !state.zkp.ephemeralKeyPair || !state.zkp.maxEpoch || !state.zkp.randomness) {
			throw new Error(
				'Start of sign-in flow could not be found. Ensure you have started the sign-in flow before calling this.',
			);
		}

		const jwt = params.get('id_token');
		if (!jwt) {
			throw new Error('Missing ID Token');
		}

		const decodedJwt = decodeJwt(jwt);
		if (!decodedJwt.sub || !decodedJwt.aud || typeof decodedJwt.aud !== 'string') {
			throw new Error('Missing JWT data');
		}

		// Verify the JWT, to ensure that we don't just get stuck with a random invalid JWT:
		await validateJWT(jwt, decodedJwt);

		const { address, salt } = await this.#enokiClient.getZkLogin({ jwt });

		this.$state.set({
			...state,
			salt,
			address,
			zkp: {
				...state.zkp,
				jwt,
			},
		});

		return params.get('state');
	}

	async restore() {
		if (this.$initialized.get()) return;

		try {
			const storedValue = await this.#store.get(this.#storeKey);
			if (!storedValue) return;
			const state: EnokiFlowState = JSON.parse(
				await this.#encryption.decrypt(this.#encryptionKey, storedValue),
			);

			// TODO: Rather than having expiration act as a logout, we should keep the state that still is relevant,
			// and just clear out the expired zkp.
			if (state.zkp?.expiresAt && Date.now() > state.zkp.expiresAt) {
				await this.logout();
			} else {
				this.$state.set(state);
			}
		} finally {
			this.$initialized.set(true);
		}
	}

	async logout() {
		this.$state.set({});
		await allTasks();
		this.#store.delete(this.#storeKey);
	}

	async getProof() {
		await this.restore();

		const state = this.$state.get();
		const { zkp, salt } = state;

		if (zkp?.proof) {
			if (zkp.expiresAt && Date.now() > zkp.expiresAt) {
				throw new Error('Stored proof is expired.');
			}

			return zkp.proof;
		}

		if (!salt || !zkp || !zkp.jwt) {
			throw new Error('Missing required parameters for proof generation');
		}

		const ephemeralKeyPair = Ed25519Keypair.fromSecretKey(fromB64(zkp.ephemeralKeyPair));

		const proof = await this.#enokiClient.createZkLoginZkp({
			jwt: zkp.jwt!,
			maxEpoch: zkp.maxEpoch!,
			randomness: zkp.randomness!,
			ephemeralPublicKey: ephemeralKeyPair.getPublicKey(),
		});

		this.$state.set({
			...state,
			zkp: {
				...zkp,
				proof,
			},
		});

		return proof;
	}

	async getKeypair() {
		// Try to restore the state if it hasn't been restored yet:
		await this.restore();

		// Get the proof, so that we ensure it exists in state:
		await this.getProof();

		// Check to see if we have the essentials for a keypair:
		const { zkp, address } = this.$state.get();
		if (!address || !zkp || !zkp.proof) {
			throw new Error('Missing required data for keypair generation.');
		}

		if (Date.now() > zkp.expiresAt) {
			throw new Error('Stored proof is expired.');
		}

		return new EnokiKeypair({
			address,
			maxEpoch: zkp.maxEpoch,
			proof: zkp.proof,
			ephemeralKeypair: Ed25519Keypair.fromSecretKey(fromB64(zkp.ephemeralKeyPair)),
		});
	}
}
