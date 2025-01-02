// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toBase64 } from '@mysten/bcs';
import { secp256r1 } from '@noble/curves/p256';
import { blake2b } from '@noble/hashes/blake2b';
import { sha256 } from '@noble/hashes/sha256';
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

type DeepPartialConfigKeys = 'rp' | 'user' | 'authenticatorSelection';

type DeepPartial<T> = T extends object
	? {
			[P in keyof T]?: DeepPartial<T[P]>;
		}
	: T;

export type BrowserPasswordProviderOptions = Pick<
	DeepPartial<PublicKeyCredentialCreationOptions>,
	DeepPartialConfigKeys
> &
	Omit<
		Partial<PublicKeyCredentialCreationOptions>,
		DeepPartialConfigKeys | 'pubKeyCredParams' | 'challenge'
	>;

export interface PasskeyProvider {
	create(): Promise<RegistrationCredential>;
	get(challenge: Uint8Array): Promise<AuthenticationCredential>;
}

// Default browser implementation
export class BrowserPasskeyProvider implements PasskeyProvider {
	#name: string;
	#options: BrowserPasswordProviderOptions;

	constructor(name: string, options: BrowserPasswordProviderOptions) {
		this.#name = name;
		this.#options = options;
	}

	async create(): Promise<RegistrationCredential> {
		return (await navigator.credentials.create({
			publicKey: {
				timeout: this.#options.timeout ?? 60000,
				...this.#options,
				rp: {
					name: this.#name,
					...this.#options.rp,
				},
				user: {
					name: this.#name,
					displayName: this.#name,
					...this.#options.user,
					id: randomBytes(10),
				},
				challenge: new TextEncoder().encode('Create passkey wallet on Sui'),
				pubKeyCredParams: [{ alg: -7, type: 'public-key' }],
				authenticatorSelection: {
					authenticatorAttachment: 'cross-platform',
					residentKey: 'required',
					requireResidentKey: true,
					userVerification: 'required',
					...this.#options.authenticatorSelection,
				},
			},
		})) as RegistrationCredential;
	}

	async get(challenge: Uint8Array): Promise<AuthenticationCredential> {
		return (await navigator.credentials.get({
			publicKey: {
				challenge,
				userVerification: this.#options.authenticatorSelection?.userVerification || 'required',
				timeout: this.#options.timeout ?? 60000,
			},
		})) as AuthenticationCredential;
	}
}

/**
 * @experimental
 * A passkey signer used for signing transactions. This is a client side implementation for [SIP-9](https://github.com/sui-foundation/sips/blob/main/sips/sip-9.md).
 */
export class PasskeyKeypair extends Signer {
	private publicKey?: Uint8Array;
	private provider: PasskeyProvider;

	/**
	 * Get the key scheme of passkey,
	 */
	getKeyScheme(): SignatureScheme {
		return 'Passkey';
	}

	/**
	 * Creates an instance of Passkey signer. If no passkey wallet had created before,
	 * use `getPasskeyInstance`. For example:
	 * ```
	 * let provider = new BrowserPasskeyProvider('Sui Passkey Example',{
	 * 	  rpName: 'Sui Passkey Example',
	 * 	  rpId: window.location.hostname,
	 * } as BrowserPasswordProviderOptions);
	 * const signer = await PasskeyKeypair.getPasskeyInstance(provider);
	 * ```
	 *
	 * If there are existing passkey wallet, use `signAndRecover` to identify the correct
	 * public key and then initialize the instance. See usage in `signAndRecover`.
	 */
	constructor(provider: PasskeyProvider, publicKey?: Uint8Array) {
		super();
		this.publicKey = publicKey;
		this.provider = provider;
	}

	/**
	 * Creates an instance of Passkey signer invoking the passkey from navigator.
	 * Note that this will invoke the passkey device to create a fresh credential.
	 * Should only be called if passkey wallet is created for the first time.
	 * todo: should rename this to `createFreshPasskeyInstance`?
	 * @param provider - the passkey provider.
	 * @returns the passkey instance.
	 */
	static async getPasskeyInstance(provider: PasskeyProvider): Promise<PasskeyKeypair> {
		// create a passkey secp256r1 with the provider.
		const credential = await provider.create();

		if (!credential.response.getPublicKey()) {
			throw new Error('Invalid credential create response');
		} else {
			const derSPKI = credential.response.getPublicKey()!;
			const pubkeyUncompressed = parseDerSPKI(new Uint8Array(derSPKI));
			const pubkey = secp256r1.ProjectivePoint.fromHex(pubkeyUncompressed);
			const pubkeyCompressed = pubkey.toRawBytes(true);
			return new PasskeyKeypair(provider, pubkeyCompressed);
		}
	}

	/**
	 * Return the public key for this passkey.
	 */
	getPublicKey(): PublicKey {
		return new PasskeyPublicKey(this.publicKey!);
	}

	/**
	 * Return the signature for the provided data (i.e. blake2b(intent_message)).
	 * This is sent to passkey as the challenge field.
	 */
	async sign(data: Uint8Array) {
		// asks the passkey to sign over challenge as the data.
		const credential = await this.provider.get(data);

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
			this.publicKey!.length !== PASSKEY_PUBLIC_KEY_SIZE
		) {
			throw new Error('Invalid signature or public key length');
		}

		// construct userSignature as flag || sig || pubkey for the secp256r1 signature.
		const arr = new Uint8Array(1 + normalized.length + this.publicKey!.length);
		arr.set([SIGNATURE_SCHEME_TO_FLAG['Secp256r1']]);
		arr.set(normalized, 1);
		arr.set(this.publicKey!, 1 + normalized.length);

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

	/**
	 * Given a message, asks the passkey device to sign it and return all (up to 4) possible public keys.
	 * See: https://bitcoin.stackexchange.com/questions/81232/how-is-public-key-extracted-from-message-digital-signature-address
	 *
	 * This is useful if the user previously created passkey wallet with the origin, but the wallet session
	 * does not have the public key / address. By calling this method twice with two different messages, the
	 * wallet can compare the returned public keys and uniquely identify the previously created passkey wallet
	 * using `findUniquePublicKey`.
	 *
	 * Alternatively, one call can be made and all possible public keys should be checked onchain to see if
	 * there is any assets.
	 *
	 * Once the correct public key is identified, a passkey instance can then be initialized with this public key.
	 *
	 * Example usage to recover wallet with two signing calls:
	 * ```
	 * let provider = new BrowserPasskeyProvider('Sui Passkey Example',{
	 *     rpName: 'Sui Passkey Example',
	 * 	   rpId: window.location.hostname,
	 * } as BrowserPasswordProviderOptions);
	 * const testMessage = new TextEncoder().encode('Hello world!');
	 * const possiblePks = await PasskeyKeypair.signAndRecover(provider, testMessage);
	 * const testMessage2 = new TextEncoder().encode('Hello world 2!');
	 * const possiblePks2 = await PasskeyKeypair.signAndRecover(provider, testMessage2);
	 * const uniquePk = findUniquePublicKey(possiblePks, possiblePks2);
	 * const signer = new PasskeyKeypair(provider, uniquePk.toRawBytes());
	 * ```
	 *
	 * @param provider - the passkey provider.
	 * @param message - the message to sign.
	 * @returns all possible public keys.
	 */
	static async signAndRecover(
		provider: PasskeyProvider,
		message: Uint8Array,
	): Promise<PublicKey[]> {
		const credential = await provider.get(message);
		const fullMessage = messageFromAssertionResponse(credential.response);
		const sig = secp256r1.Signature.fromDER(new Uint8Array(credential.response.signature));

		const res = [];
		for (let i = 0; i < 4; i++) {
			const s = sig.addRecoveryBit(i);
			try {
				const pubkey = s.recoverPublicKey(sha256(fullMessage));
				const pk = new PasskeyPublicKey(pubkey.toRawBytes(true));
				res.push(pk);
			} catch {
				continue;
			}
		}
		return res;
	}
}

/**
 * Finds the unique public key that exists in both arrays, throws error if the common
 * pubkey does not equal to one.
 *
 * @param arr1 - The first pubkeys array.
 * @param arr2 - The second pubkeys array.
 * @returns The only common pubkey in both arrays.
 */
export function findUniquePublicKey(arr1: PublicKey[], arr2: PublicKey[]): PublicKey {
	const matchingPubkeys: PublicKey[] = [];
	for (const pubkey1 of arr1) {
		for (const pubkey2 of arr2) {
			if (pubkey1.equals(pubkey2)) {
				matchingPubkeys.push(pubkey1);
			}
		}
	}
	if (matchingPubkeys.length !== 1) {
		throw new Error('No unique public key found');
	}
	return matchingPubkeys[0];
}

/**
 * Constructs the message that the passkey signature is produced over as authenticatorData || sha256(clientDataJSON).
 */
function messageFromAssertionResponse(response: AuthenticatorAssertionResponse): Uint8Array {
	const authenticatorData = new Uint8Array(response.authenticatorData);
	const clientDataJSON = new Uint8Array(response.clientDataJSON);
	const clientDataJSONDigest = sha256(clientDataJSON);
	return new Uint8Array([...authenticatorData, ...clientDataJSONDigest]);
}
