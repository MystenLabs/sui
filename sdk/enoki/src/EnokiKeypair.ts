// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Keypair } from '@mysten/sui/cryptography';
import type { SignatureWithBytes } from '@mysten/sui/cryptography';
import type { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { ZkLoginPublicIdentifier } from '@mysten/sui/zklogin';
import type { ZkLoginSignatureInputs } from '@mysten/sui/zklogin';
import { getZkLoginSignature } from '@mysten/zklogin';

export class EnokiPublicKey extends ZkLoginPublicIdentifier {}

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

		this.#publicKey = new EnokiPublicKey(
			ZkLoginPublicIdentifier.fromSignatureInputs(input.proof).toRawBytes(),
		);
	}

	async sign(data: Uint8Array) {
		return this.#ephemeralKeypair.sign(data);
	}

	async signPersonalMessage(bytes: Uint8Array): Promise<SignatureWithBytes> {
		const { bytes: signedBytes, signature: userSignature } =
			await this.#ephemeralKeypair.signPersonalMessage(bytes);

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

	async signTransaction(bytes: Uint8Array): Promise<SignatureWithBytes> {
		const { bytes: signedBytes, signature: userSignature } =
			await this.#ephemeralKeypair.signTransaction(bytes);

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

	/** @deprecated This method always throws and was only added to implement the Keypair interface, future version of this class will extend Signer instead */
	export(): never {
		throw new Error('EnokiKeypair does not support exporting');
	}

	/** @deprecated This method always throws and was only added to implement the Keypair interface, future version of this class will extend Signer instead */
	getSecretKey(): never {
		throw new Error('EnokiKeypair does not support getting the secret key');
	}
}
