// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toBase64 } from '@mysten/bcs';
import { secp256r1 } from '@noble/curves/p256';
import { blake2b } from '@noble/hashes/blake2b';
import { randomBytes } from '@noble/hashes/utils';
import type {
	AuthenticationCredential,
	RegistrationCredential,
} from '@simplewebauthn/typescript-types';

import { PasskeyAuthenticator } from '../../bcs/bcs.js';
import type { IntentScope, SignatureWithBytes } from '../../cryptography/index.js';
import { messageWithIntent, SIGNATURE_SCHEME_TO_FLAG, Signer } from '../../cryptography/index.js';
import type { PublicKey } from '../../cryptography/publickey.js';
import type { SignatureScheme } from '../../cryptography/signature-scheme.js';
import {
	parseDerSPKI,
	PASSKEY_PUBLIC_KEY_SIZE,
	PASSKEY_SIGNATURE_SIZE,
	PasskeyPublicKey,
} from './publickey.js';

export interface PasskeyCreateOptions {
	timeout: number; // The timeout for the passkey creation dialog.
	name: string; // The name of the user
	displayName: string; // The display name of the user, this is shown at creation dialog
	rpName: string; // The name of the relying party, this is shown to user in the passkey creation dialog.
}

export interface PasskeySignOptions {
	timeout: number; // The timeout for the passkey authentication dialog.
}

function validateCreateOptions(options: PasskeyCreateOptions) {
	if (options.timeout <= 0) throw new Error('Timeout must be positive');
	if (!options.displayName?.trim()) throw new Error('Display name cannot be empty');
	if (!options.rpName?.trim()) throw new Error('RP name cannot be empty');
	if (options.timeout > 300000) throw new Error('Timeout cannot exceed 5 minutes');
}

export interface PasskeyProvider {
	options: PasskeyCreateOptions;
	create(options: PasskeyCreateOptions): Promise<RegistrationCredential>;
	get(challenge: Uint8Array, timeout: number): Promise<AuthenticationCredential>;
}

// Default browser implementation
export class BrowserPasskeyProvider implements PasskeyProvider {
	options: PasskeyCreateOptions;
	constructor(options: PasskeyCreateOptions) {
		validateCreateOptions(options);
		this.options = options;
	}

	async create(): Promise<RegistrationCredential> {
		return (await navigator.credentials.create({
			publicKey: {
				challenge: new TextEncoder().encode('Create passkey wallet on Sui'),
				rp: {
					name: this.options.rpName,
				},
				user: {
					id: randomBytes(10),
					name: this.options.name,
					displayName: this.options.displayName,
				},
				pubKeyCredParams: [{ alg: -7, type: 'public-key' }],
				authenticatorSelection: {
					authenticatorAttachment: 'cross-platform',
					residentKey: 'required',
					requireResidentKey: true,
					userVerification: 'required',
				},
				timeout: this.options.timeout,
			},
		})) as RegistrationCredential;
	}

	async get(challenge: Uint8Array, timeout: number): Promise<AuthenticationCredential> {
		return (await navigator.credentials.get({
			publicKey: {
				challenge,
				userVerification: 'required',
				timeout,
			},
		})) as AuthenticationCredential;
	}
}

/**
 * A passkey signer used for signing transactions. This is a client side implementation for [SIP-9](https://github.com/sui-foundation/sips/blob/main/sips/sip-9.md).
 */
export class PasskeyKeypair extends Signer {
	private publicKey: Uint8Array;
	private provider: PasskeyProvider;

	/**
	 * Get the key scheme of passkey,
	 */
	getKeyScheme(): SignatureScheme {
		return 'Passkey';
	}

	/**
	 * Creates an instance of Passkey signer. It's expected to call the static `getPasskeyInstance` method to create an instance.
	 * For example:
	 * ```
	 * const signer = await PasskeyKeypair.getPasskeyInstance();
	 * ```
	 */
	constructor(publicKey: Uint8Array, provider: PasskeyProvider) {
		super();
		this.publicKey = publicKey;
		this.provider = provider;
	}

	/**
	 * Creates an instance of Passkey signer invoking the passkey from navigator.
	 */
	static async getPasskeyInstance(
		provider = new BrowserPasskeyProvider({
			timeout: 60000,
			displayName: 'Sui Wallet User',
			rpName: 'Sui Wallet',
			name: 'Sui Wallet User',
		}),
	): Promise<PasskeyKeypair> {
		// create a passkey secp256r1 with the provider.
		const credential = await provider.create();

		if (!credential.response.getPublicKey) {
			throw new Error('Invalid credential createresponse');
		} else {
			const derSPKI = credential.response.getPublicKey()!;
			const pubkeyUncompressed = parseDerSPKI(new Uint8Array(derSPKI));
			const pubkey = secp256r1.ProjectivePoint.fromHex(pubkeyUncompressed);
			const pubkeyCompressed = pubkey.toRawBytes(true);
			return new PasskeyKeypair(pubkeyCompressed, provider);
		}
	}

	/**
	 * Return the public key for this passkey.
	 */
	getPublicKey(): PublicKey {
		return new PasskeyPublicKey(this.publicKey);
	}

	/**
	 * Return the signature for the provided data (i.e. blake2b(intent_message)).
	 * This is sent to passkey as the challenge field.
	 */
	async sign(data: Uint8Array, options: PasskeySignOptions = { timeout: 60000 }) {
		// sendss the passkey to sign over challenge as the data.
		const credential = await this.provider.get(data, options.timeout);

		// parse authenticatorData (as bytes), clientDataJSON (decoded as string).
		const authenticatorData = new Uint8Array(credential.response.authenticatorData);
		const clientDataJSON = new Uint8Array(credential.response.clientDataJSON); // response.clientDataJSON is already UTF-8 encoded JSON
		const decoder = new TextDecoder();
		const clientDataJSONString: string = decoder.decode(clientDataJSON);

		// parse the signature from DER format, normalize and convert to compressed format (33 bytes).
		const sig = secp256r1.Signature.fromDER(new Uint8Array(credential.response.signature));
		const normalized = sig.normalizeS().toCompactRawBytes();

		if (
			normalized.length !== PASSKEY_SIGNATURE_SIZE ||
			this.publicKey.length !== PASSKEY_PUBLIC_KEY_SIZE
		) {
			throw new Error('Invalid signature or public key length');
		}

		// construct userSignature as flag || sig || pubkey for the secp256r1 signature.
		const arr = new Uint8Array(1 + normalized.length + this.publicKey.length);
		arr.set([SIGNATURE_SCHEME_TO_FLAG['Secp256r1']]);
		arr.set(normalized, 1);
		arr.set(this.publicKey, 1 + normalized.length);

		// serialize all fields into a passkey signature according to https://github.com/sui-foundation/sips/blob/main/sips/sip-9.md#signature-encoding
		return PasskeyAuthenticator.serialize({
			authenticatorData: authenticatorData,
			clientDataJson: clientDataJSONString,
			userSignature: arr,
		}).toBytes();
	}

	/**
	 * This overrides the base class implementation that accepts the raw bytes and signs its
	 * digest of the intent message, then serialize it with the passkey flag.
	 */
	async signWithIntent(bytes: Uint8Array, intent: IntentScope): Promise<SignatureWithBytes> {
		// prepend it into an intent message and computes the digest.
		const intentMessage = messageWithIntent(intent, bytes);
		const digest = blake2b(intentMessage, { dkLen: 32 });

		// sign the digest.
		const signature = await this.sign(digest);

		// prepend with the passkey flag.
		const serializedSignature = new Uint8Array(1 + signature.length);
		serializedSignature.set([SIGNATURE_SCHEME_TO_FLAG[this.getKeyScheme()]]);
		serializedSignature.set(signature, 1);
		return {
			signature: toBase64(serializedSignature),
			bytes: toBase64(bytes),
		};
	}
}
