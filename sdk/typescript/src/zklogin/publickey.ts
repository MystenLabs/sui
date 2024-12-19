// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromBase64, toBase64, toHex } from '@mysten/bcs';
import { blake2b } from '@noble/hashes/blake2b';
import { bytesToHex } from '@noble/hashes/utils';

import { PublicKey } from '../cryptography/publickey.js';
import type { PublicKeyInitData } from '../cryptography/publickey.js';
import { SIGNATURE_SCHEME_TO_FLAG } from '../cryptography/signature-scheme.js';
import { SuiGraphQLClient } from '../graphql/client.js';
import { graphql } from '../graphql/schemas/latest/index.js';
import { normalizeSuiAddress, SUI_ADDRESS_LENGTH } from '../utils/sui-types.js';
import { extractClaimValue } from './jwt-utils.js';
import { parseZkLoginSignature } from './signature.js';
import { toBigEndianBytes, toPaddedBigEndianBytes } from './utils.js';

/**
 * A zkLogin public identifier
 */
export class ZkLoginPublicIdentifier extends PublicKey {
	#data: Uint8Array;
	#client?: SuiGraphQLClient;
	#legacyAddress: boolean;

	/**
	 * Create a new ZkLoginPublicIdentifier object
	 * @param value zkLogin public identifier as buffer or base-64 encoded string
	 */
	constructor(value: PublicKeyInitData, { client }: { client?: SuiGraphQLClient } = {}) {
		super();

		this.#client = client;

		if (typeof value === 'string') {
			this.#data = fromBase64(value);
		} else if (value instanceof Uint8Array) {
			this.#data = value;
		} else {
			this.#data = Uint8Array.from(value);
		}
		this.#legacyAddress = this.#data.length !== this.#data[0] + 1 + 32;

		if (this.#legacyAddress) {
			this.#data = normalizeZkLoginPublicKeyBytes(this.#data);
		}
	}

	/**
	 * Checks if two zkLogin public identifiers are equal
	 */
	override equals(publicKey: ZkLoginPublicIdentifier): boolean {
		return super.equals(publicKey);
	}

	override toSuiAddress(): string {
		if (this.#legacyAddress) {
			return this.#toLegacyAddress();
		}

		return super.toSuiAddress();
	}

	#toLegacyAddress() {
		const legacyBytes = normalizeZkLoginPublicKeyBytes(this.#data, true);
		const addressBytes = new Uint8Array(legacyBytes.length + 1);
		addressBytes[0] = this.flag();
		addressBytes.set(legacyBytes, 1);
		return normalizeSuiAddress(
			bytesToHex(blake2b(addressBytes, { dkLen: 32 })).slice(0, SUI_ADDRESS_LENGTH * 2),
		);
	}

	/**
	 * Return the byte array representation of the zkLogin public identifier
	 */
	toRawBytes(): Uint8Array {
		return this.#data;
	}

	/**
	 * Return the Sui address associated with this ZkLogin public identifier
	 */
	flag(): number {
		return SIGNATURE_SCHEME_TO_FLAG['ZkLogin'];
	}

	/**
	 * Verifies that the signature is valid for for the provided message
	 */
	async verify(_message: Uint8Array, _signature: Uint8Array | string): Promise<boolean> {
		throw Error('does not support');
	}

	/**
	 * Verifies that the signature is valid for for the provided PersonalMessage
	 */
	verifyPersonalMessage(message: Uint8Array, signature: Uint8Array | string): Promise<boolean> {
		const parsedSignature = parseSerializedZkLoginSignature(signature);
		const address = new ZkLoginPublicIdentifier(parsedSignature.publicKey).toSuiAddress();

		return graphqlVerifyZkLoginSignature({
			address: address,
			bytes: toBase64(message),
			signature: parsedSignature.serializedSignature,
			intentScope: 'PERSONAL_MESSAGE',
			client: this.#client,
		});
	}

	/**
	 * Verifies that the signature is valid for for the provided Transaction
	 */
	verifyTransaction(transaction: Uint8Array, signature: Uint8Array | string): Promise<boolean> {
		const parsedSignature = parseSerializedZkLoginSignature(signature);
		const address = new ZkLoginPublicIdentifier(parsedSignature.publicKey).toSuiAddress();
		return graphqlVerifyZkLoginSignature({
			address: address,
			bytes: toBase64(transaction),
			signature: parsedSignature.serializedSignature,
			intentScope: 'TRANSACTION_DATA',
			client: this.#client,
		});
	}

	/**
	 * Verifies that the public key is associated with the provided address
	 */
	override verifyAddress(address: string): boolean {
		return address === super.toSuiAddress() || address === this.#toLegacyAddress();
	}
}

// Derive the public identifier for zklogin based on address seed and iss.
export function toZkLoginPublicIdentifier(
	addressSeed: bigint,
	iss: string,
	options?: { client?: SuiGraphQLClient; legacyAddress?: boolean },
): ZkLoginPublicIdentifier {
	// Consists of iss_bytes_len || iss_bytes || padded_32_byte_address_seed.
	const addressSeedBytesBigEndian = options?.legacyAddress
		? toBigEndianBytes(addressSeed, 32)
		: toPaddedBigEndianBytes(addressSeed, 32);

	const issBytes = new TextEncoder().encode(iss);
	const tmp = new Uint8Array(1 + issBytes.length + addressSeedBytesBigEndian.length);
	tmp.set([issBytes.length], 0);
	tmp.set(issBytes, 1);
	tmp.set(addressSeedBytesBigEndian, 1 + issBytes.length);
	return new ZkLoginPublicIdentifier(tmp, options);
}

const VerifyZkLoginSignatureQuery = graphql(`
	query Zklogin(
		$bytes: Base64!
		$signature: Base64!
		$intentScope: ZkLoginIntentScope!
		$author: SuiAddress!
	) {
		verifyZkloginSignature(
			bytes: $bytes
			signature: $signature
			intentScope: $intentScope
			author: $author
		) {
			success
			errors
		}
	}
`);

function normalizeZkLoginPublicKeyBytes(bytes: Uint8Array, legacyAddress = false) {
	const issByteLength = bytes[0] + 1;
	const addressSeed = BigInt(`0x${toHex(bytes.slice(issByteLength))}`);
	const seedBytes = legacyAddress
		? toBigEndianBytes(addressSeed, 32)
		: toPaddedBigEndianBytes(addressSeed, 32);
	const data = new Uint8Array(issByteLength + seedBytes.length);
	data.set(bytes.slice(0, issByteLength), 0);
	data.set(seedBytes, issByteLength);
	return data;
}

async function graphqlVerifyZkLoginSignature({
	address,
	bytes,
	signature,
	intentScope,
	client = new SuiGraphQLClient({
		url: 'https://sui-mainnet.mystenlabs.com/graphql',
	}),
}: {
	address: string;
	bytes: string;
	signature: string;
	intentScope: 'PERSONAL_MESSAGE' | 'TRANSACTION_DATA';
	client?: SuiGraphQLClient;
}) {
	const resp = await client.query({
		query: VerifyZkLoginSignatureQuery,
		variables: {
			bytes,
			signature,
			intentScope,
			author: address,
		},
	});

	return (
		resp.data?.verifyZkloginSignature.success === true &&
		resp.data?.verifyZkloginSignature.errors.length === 0
	);
}

export function parseSerializedZkLoginSignature(signature: Uint8Array | string) {
	const bytes = typeof signature === 'string' ? fromBase64(signature) : signature;

	if (bytes[0] !== SIGNATURE_SCHEME_TO_FLAG.ZkLogin) {
		throw new Error('Invalid signature scheme');
	}

	const signatureBytes = bytes.slice(1);
	const { inputs, maxEpoch, userSignature } = parseZkLoginSignature(signatureBytes);
	const { issBase64Details, addressSeed } = inputs;
	const iss = extractClaimValue<string>(issBase64Details, 'iss');
	const publicIdentifer = toZkLoginPublicIdentifier(BigInt(addressSeed), iss);
	return {
		serializedSignature: toBase64(bytes),
		signatureScheme: 'ZkLogin' as const,
		zkLogin: {
			inputs,
			maxEpoch,
			userSignature,
			iss,
			addressSeed: BigInt(addressSeed),
		},
		signature: bytes,
		publicKey: publicIdentifer.toRawBytes(),
	};
}
