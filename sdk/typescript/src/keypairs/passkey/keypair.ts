// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toBase64, toHex } from '@mysten/bcs';
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

/**
 * A passkey signer used for signing transactions. This is a client side implementation for [SIP-9](https://github.com/sui-foundation/sips/blob/main/sips/sip-9.md).
 */
export class PasskeyKeypair extends Signer {
	private publicKey: Uint8Array;

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
	 * const signer = await AwsKmsSigner.getPasskeyInstance();
	 * ```
	 */
	constructor(publicKey: Uint8Array) {
		super();
		this.publicKey = publicKey;
	}

	/**
	 * Creates an instance of Passkey signer invoking the passkey from navigator.
	 */
	static async getPasskeyInstance(): Promise<PasskeyKeypair> {
		// create a passkey secp256r1 key for the relying party.
		const credential = (await navigator.credentials.create({
			publicKey: {
				challenge: new TextEncoder().encode('Create passkey wallet on Sui'),
				rp: {
					name: 'Passkey Wallet on Sui',
				},
				user: {
					id: randomBytes(10),
					name: 'sui-wallet-user',
					displayName: 'Sui Wallet User',
				},
				pubKeyCredParams: [{ alg: -7, type: 'public-key' }], // -7 is ES256
				authenticatorSelection: {
					authenticatorAttachment: 'cross-platform',
					residentKey: 'required',
					requireResidentKey: true,
					userVerification: 'required',
				},
				timeout: 60000,
			},
		})) as RegistrationCredential;

		if (!credential.response.getPublicKey) {
			throw new Error('Invalid credential createresponse');
		} else {
			const derSPKI = credential.response.getPublicKey()!;
			const pubkeyUncompressed = parseDerSPKI(new Uint8Array(derSPKI));
			const pubkey = secp256r1.ProjectivePoint.fromHex(pubkeyUncompressed);
			const pubkeyCompressed = pubkey.toRawBytes(true);
			return new PasskeyKeypair(pubkeyCompressed);
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
	async sign(data: Uint8Array) {
		// sents the passkey to sign over challenge as the data.
		const credential = (await navigator.credentials.get({
			publicKey: {
				challenge: data,
				userVerification: 'required',
				timeout: 60000,
			},
		})) as AuthenticationCredential;

		// parse authenticatorData (as bytes), clientDataJSON (decoded as string).
		const authenticatorData = new Uint8Array(credential.response.authenticatorData);
		const clientDataJSON = new Uint8Array(credential.response.clientDataJSON); // response.clientDataJSON is already UTF-8 encoded JSON
		const decoder = new TextDecoder();
		const clientDataJSONString: string = decoder.decode(clientDataJSON);

		// parse the signature from DER format, normalize and convert to compressed format (33 bytes).
		const sig = secp256r1.Signature.fromDER(new Uint8Array(credential.response.signature));
		let normalized = sig.normalizeS().toCompactRawBytes();
		const compressedPubkey = secp256r1.ProjectivePoint.fromHex(toHex(this.publicKey)).toRawBytes(
			true,
		);

		if (
			normalized.length !== PASSKEY_SIGNATURE_SIZE ||
			compressedPubkey.length !== PASSKEY_PUBLIC_KEY_SIZE
		) {
			throw new Error('Invalid signature or public key length');
		}

		// construct userSignature as flag || sig || pubkey for the secp256r1 signature.
		const arr = new Uint8Array(1 + normalized.length + compressedPubkey.length);
		arr.set([SIGNATURE_SCHEME_TO_FLAG['Secp256r1']]);
		arr.set(normalized, 1);
		arr.set(compressedPubkey, 1 + normalized.length);

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
