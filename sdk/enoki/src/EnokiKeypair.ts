// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SignatureWithBytes } from '@mysten/sui.js/cryptography';
import { Keypair, PublicKey, SIGNATURE_SCHEME_TO_FLAG } from '@mysten/sui.js/cryptography';
import type { Ed25519Keypair, Ed25519PublicKey } from '@mysten/sui.js/keypairs/ed25519';
import type { ZkLoginSignatureInputs } from '@mysten/sui.js/zklogin';
import { getZkLoginSignature } from '@mysten/zklogin';

export class EnokiPublicKey extends PublicKey {
	#address: string;
	#ephemeralPublicKey: Ed25519PublicKey;

	constructor(input: { address: string; ephemeralPublicKey: Ed25519PublicKey }) {
		super();
		this.#address = input.address;
		this.#ephemeralPublicKey = input.ephemeralPublicKey;
	}

	flag(): number {
		return SIGNATURE_SCHEME_TO_FLAG['ZkLogin'];
	}

	toSuiAddress(): string {
		return this.#address;
	}

	verify(): never {
		throw new Error('Verification for EnokiPublicKey is not supported');
	}

	toRawBytes(): Uint8Array {
		return this.#ephemeralPublicKey.toRawBytes();
	}
}

export class EnokiKeypair extends Keypair {
	#proof: ZkLoginSignatureInputs;
	#maxEpoch: number;
	#ephemeralKeypair: Ed25519Keypair;
	#publicKey: EnokiPublicKey;

	constructor(input: {
		address: string;
		maxEpoch: number;
		proof: ZkLoginSignatureInputs;
		ephemeralKeypair: Ed25519Keypair;
	}) {
		super();
		this.#proof = input.proof;
		this.#maxEpoch = input.maxEpoch;
		this.#ephemeralKeypair = input.ephemeralKeypair;
		this.#publicKey = new EnokiPublicKey({
			address: input.address,
			ephemeralPublicKey: input.ephemeralKeypair.getPublicKey(),
		});
	}

	signData(data: Uint8Array) {
		return this.#ephemeralKeypair.signData(data);
	}

	async sign(data: Uint8Array) {
		return this.#ephemeralKeypair.sign(data);
	}

	signPersonalMessage(): never {
		throw new Error('Signing personal messages is not supported');
	}

	async signTransactionBlock(bytes: Uint8Array): Promise<SignatureWithBytes> {
		const { bytes: signedBytes, signature: userSignature } =
			await this.#ephemeralKeypair.signTransactionBlock(bytes);

		const zkSignature = getZkLoginSignature({
			inputs: this.#proof,
			maxEpoch: this.#maxEpoch,
			userSignature,
		});

		return {
			bytes: signedBytes,
			signature: zkSignature,
		};
	}

	getKeyScheme() {
		return this.#ephemeralKeypair.getKeyScheme();
	}

	getPublicKey() {
		return this.#publicKey;
	}

	export(): never {
		throw new Error('EnokiKeypair does not support exporting');
	}

	getSecretKey(): never {
		throw new Error('EnokiKeypair does not support get secret key');
	}
}
