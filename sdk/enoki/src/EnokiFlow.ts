// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiClient } from '@mysten/sui.js/client';
import { decodeSuiPrivateKey } from '@mysten/sui.js/cryptography';
import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import type { TransactionBlock } from '@mysten/sui.js/transactions';
import { fromB64, toB64 } from '@mysten/sui.js/utils';
import type { ZkLoginSignatureInputs } from '@mysten/sui.js/zklogin';
import { decodeJwt } from 'jose';
import type { WritableAtom } from 'nanostores';
import { atom, onMount, onSet } from 'nanostores';

import type { Encryption } from './encryption.js';
import { createDefaultEncryption } from './encryption.js';
import type { EnokiClientConfig } from './EnokiClient/index.js';
import { EnokiClient } from './EnokiClient/index.js';
import { EnokiKeypair } from './EnokiKeypair.js';
import type { SyncStore } from './stores.js';
import { createSessionStorage } from './stores.js';

export interface EnokiFlowConfig extends EnokiClientConfig {
	/**
	 * The storage interface to persist Enoki data locally.
	 * If not provided, it will use a sessionStorage-backed store.
	 */
	store?: SyncStore;
	/**
	 * The encryption interface that will be used to encrypt data before storing it locally.
	 * If not provided, it will use a default encryption interface.
	 */
	encryption?: Encryption;
}

// State that is not bound to a session, and is encrypted.
export interface ZkLoginState {
	provider?: AuthProvider;
	address?: string;
	salt?: string;
}

// State that session-bound, and is encrypted in storage.
export interface ZkLoginSession {
	ephemeralKeyPair: string;
	maxEpoch: number;
	randomness: string;
	expiresAt: number;

	jwt?: string;
	proof?: ZkLoginSignatureInputs;
}

export type AuthProvider = 'google' | 'facebook' | 'twitch';

const createStorageKeys = (apiKey: string) => ({
	STATE: `@enoki/flow/state/${apiKey}`,
	SESSION: `@enoki/flow/session/${apiKey}`,
});

export class EnokiFlow {
	#storageKeys: { STATE: string; SESSION: string };
	#enokiClient: EnokiClient;
	#encryption: Encryption;
	#encryptionKey: string;
	#store: SyncStore;

	$zkLoginSession: WritableAtom<{ initialized: boolean; value: ZkLoginSession | null }>;
	$zkLoginState: WritableAtom<ZkLoginState>;

	constructor(config: EnokiFlowConfig) {
		this.#enokiClient = new EnokiClient({
			apiKey: config.apiKey,
			apiUrl: config.apiUrl,
		});
		this.#encryptionKey = config.apiKey;
		this.#encryption = config.encryption ?? createDefaultEncryption();
		this.#store = config.store ?? createSessionStorage();
		this.#storageKeys = createStorageKeys(config.apiKey);

		let storedState = null;
		try {
			const rawStoredValue = this.#store.get(this.#storageKeys.STATE);
			if (rawStoredValue) {
				storedState = JSON.parse(rawStoredValue);
			}
		} catch {
			// Ignore errors
		}

		this.$zkLoginState = atom(storedState || {});
		this.$zkLoginSession = atom({ initialized: false, value: null });

		// Hydrate the session on mount:
		onMount(this.$zkLoginSession, () => {
			this.getSession();
		});

		onSet(this.$zkLoginState, ({ newValue }) => {
			this.#store.set(this.#storageKeys.STATE, JSON.stringify(newValue));
		});
	}

	get enokiClient() {
		return this.#enokiClient;
	}

	async createAuthorizationURL(input: {
		provider: AuthProvider;
		clientId: string;
		redirectUrl: string;
		extraParams?: Record<string, unknown>;
	}) {
		const ephemeralKeyPair = new Ed25519Keypair();
		const { nonce, randomness, maxEpoch, estimatedExpiration } =
			await this.#enokiClient.createZkLoginNonce({
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

		this.$zkLoginState.set({ provider: input.provider });
		await this.#setSession({
			expiresAt: estimatedExpiration,
			maxEpoch,
			randomness,
			ephemeralKeyPair: toB64(decodeSuiPrivateKey(ephemeralKeyPair.getSecretKey()).secretKey),
		});

		return oauthUrl;
	}

	// TODO: Should our SDK manage this automatically in addition to exposing a method?
	async handleAuthCallback(hash: string = window.location.hash) {
		const params = new URLSearchParams(hash.startsWith('#') ? hash.slice(1) : hash);

		// Before we handle the auth redirect and get the state, we need to restore it:
		const zkp = await this.getSession();

		if (!zkp || !zkp.ephemeralKeyPair || !zkp.maxEpoch || !zkp.randomness) {
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

		const { address, salt } = await this.#enokiClient.getZkLogin({ jwt });

		this.$zkLoginState.set({
			...this.$zkLoginState.get(),
			salt,
			address,
		});
		await this.#setSession({
			...zkp,
			jwt,
		});

		return params.get('state');
	}

	async #setSession(newValue: ZkLoginSession | null) {
		if (newValue) {
			const storedValue = await this.#encryption.encrypt(
				this.#encryptionKey,
				JSON.stringify(newValue),
			);

			this.#store.set(this.#storageKeys.SESSION, storedValue);
		} else {
			this.#store.delete(this.#storageKeys.SESSION);
		}

		this.$zkLoginSession.set({ initialized: true, value: newValue });
	}

	async getSession() {
		if (this.$zkLoginSession.get().initialized) {
			return this.$zkLoginSession.get().value;
		}

		try {
			const storedValue = this.#store.get(this.#storageKeys.SESSION);
			if (!storedValue) return null;

			const state: ZkLoginSession = JSON.parse(
				await this.#encryption.decrypt(this.#encryptionKey, storedValue),
			);

			// TODO: Rather than having expiration act as a logout, we should keep the state that still is relevant,
			// and just clear out the expired session, but keep the other zkLogin state.
			if (state?.expiresAt && Date.now() > state.expiresAt) {
				await this.logout();
			} else {
				this.$zkLoginSession.set({ initialized: true, value: state });
			}
		} catch {
			this.$zkLoginSession.set({ initialized: true, value: null });
		}

		return this.$zkLoginSession.get().value;
	}

	async logout() {
		this.$zkLoginState.set({});
		this.#store.delete(this.#storageKeys.STATE);

		await this.#setSession(null);
	}

	// TODO: Should this return the proof if it already exists?
	async getProof() {
		const zkp = await this.getSession();
		const { salt } = this.$zkLoginState.get();

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
			jwt: zkp.jwt,
			maxEpoch: zkp.maxEpoch,
			randomness: zkp.randomness,
			ephemeralPublicKey: ephemeralKeyPair.getPublicKey(),
		});

		await this.#setSession({
			...zkp,
			proof,
		});

		return proof;
	}

	async getKeypair() {
		// Get the proof, so that we ensure it exists in state:
		await this.getProof();

		const zkp = await this.getSession();

		// Check to see if we have the essentials for a keypair:
		const { address } = this.$zkLoginState.get();
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

	async sponsorTransactionBlock({
		network,
		transactionBlock,
		client,
	}: {
		network?: 'mainnet' | 'testnet';
		transactionBlock: TransactionBlock;
		client: SuiClient;
	}) {
		const session = await this.getSession();

		if (!session || !session.jwt) {
			throw new Error('Missing required data for sponsorship.');
		}

		const transactionBlockKindBytes = await transactionBlock.build({
			onlyTransactionKind: true,
			client,
			// Theses limits will get verified during the final transaction construction, so we can safely ignore them here:
			limits: {
				maxGasObjects: Infinity,
				maxPureArgumentSize: Infinity,
				maxTxGas: Infinity,
				maxTxSizeBytes: Infinity,
			},
		});

		return await this.#enokiClient.createSponsoredTransactionBlock({
			jwt: session.jwt,
			network,
			transactionBlockKindBytes: toB64(transactionBlockKindBytes),
		});
	}

	async executeTransactionBlock({
		bytes,
		digest,
		client,
	}: {
		bytes: string;
		digest: string;
		client: SuiClient;
	}) {
		const keypair = await this.getKeypair();
		const userSignature = await keypair.signTransactionBlock(fromB64(bytes));

		await this.#enokiClient.executeSponsoredTransactionBlock({
			digest,
			signature: userSignature.signature,
		});

		// TODO: Should the parent just do this?
		await client.waitForTransactionBlock({ digest });

		return { digest };
	}

	async sponsorAndExecuteTransactionBlock({
		network,
		transactionBlock,
		client,
	}: {
		network?: 'mainnet' | 'testnet';
		transactionBlock: TransactionBlock;
		client: SuiClient;
	}) {
		const { bytes, digest } = await this.sponsorTransactionBlock({
			network,
			transactionBlock,
			client,
		});
		return await this.executeTransactionBlock({ bytes, digest, client });
	}
}
