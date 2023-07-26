// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toB64 } from '@mysten/bcs';
import { IntentScope, messageWithIntent } from './intent.js';
import { blake2b } from '@noble/hashes/blake2b';
import { bcs } from '../bcs/index.js';
import type { SerializedSignature } from './index.js';
import { SUI_ADDRESS_LENGTH, normalizeSuiAddress } from '../utils/sui-types.js';
import { bytesToHex } from '@noble/hashes/utils';

/**
 * Value to be converted into public key.
 */
export type PublicKeyInitData = string | Uint8Array | Iterable<number>;

export function bytesEqual(a: Uint8Array, b: Uint8Array) {
	if (a === b) return true;

	if (a.length !== b.length) {
		return false;
	}

	for (let i = 0; i < a.length; i++) {
		if (a[i] !== b[i]) {
			return false;
		}
	}
	return true;
}

/**
 * A public key
 */
export abstract class PublicKey {
	/**
	 * Checks if two public keys are equal
	 */
	equals(publicKey: PublicKey) {
		return bytesEqual(this.toRawBytes(), publicKey.toRawBytes());
	}

	/**
	 * Return the base-64 representation of the public key
	 */
	toBase64() {
		return toB64(this.toRawBytes());
	}

	/**
	 * @deprecated use toBase64 instead.
	 *
	 * Return the base-64 representation of the public key
	 */
	toString() {
		return this.toBase64();
	}

	/**
	 * Return the Sui representation of the public key encoded in
	 * base-64. A Sui public key is formed by the concatenation
	 * of the scheme flag with the raw bytes of the public key
	 */
	toSuiPublicKey(): string {
		const bytes = this.toSuiBytes();
		return toB64(bytes);
	}

	verifyWithIntent(
		bytes: Uint8Array,
		signature: Uint8Array | SerializedSignature,
		intent: IntentScope,
	): Promise<boolean> {
		const intentMessage = messageWithIntent(intent, bytes);
		const digest = blake2b(intentMessage, { dkLen: 32 });

		return this.verify(digest, signature);
	}

	/**
	 * Verifies that the signature is valid for for the provided PersonalMessage
	 */
	verifyPersonalMessage(
		message: Uint8Array,
		signature: Uint8Array | SerializedSignature,
	): Promise<boolean> {
		return this.verifyWithIntent(
			bcs.ser(['vector', 'u8'], message).toBytes(),
			signature,
			IntentScope.PersonalMessage,
		);
	}

	/**
	 * Verifies that the signature is valid for for the provided TransactionBlock
	 */
	verifyTransactionBlock(
		transactionBlock: Uint8Array,
		signature: Uint8Array | SerializedSignature,
	): Promise<boolean> {
		return this.verifyWithIntent(transactionBlock, signature, IntentScope.TransactionData);
	}

	/**
	 * Returns the bytes representation of the public key
	 * prefixed with the signature scheme flag
	 */
	toSuiBytes(): Uint8Array {
		const rawBytes = this.toRawBytes();
		const suiBytes = new Uint8Array(rawBytes.length + 1);
		suiBytes.set([this.flag()]);
		suiBytes.set(rawBytes, 1);

		return suiBytes;
	}

	/**
	 * @deprecated use `toRawBytes` instead.
	 */
	toBytes() {
		return this.toRawBytes();
	}

	/**
	 * Return the Sui address associated with this Ed25519 public key
	 */
	toSuiAddress(): string {
		// Each hex char represents half a byte, hence hex address doubles the length
		return normalizeSuiAddress(
			bytesToHex(blake2b(this.toSuiBytes(), { dkLen: 32 })).slice(0, SUI_ADDRESS_LENGTH * 2),
		);
	}

	/**
	 * Return the byte array representation of the public key
	 */
	abstract toRawBytes(): Uint8Array;

	/**
	 * Return signature scheme flag of the public key
	 */
	abstract flag(): number;

	/**
	 * Verifies that the signature is valid for for the provided message
	 */
	abstract verify(data: Uint8Array, signature: Uint8Array | SerializedSignature): Promise<boolean>;
}
