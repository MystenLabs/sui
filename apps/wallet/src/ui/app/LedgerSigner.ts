// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiClient } from '@mysten/sui.js/client';
import {
	type SerializedSignature,
	toSerializedSignature,
	type SignatureScheme,
} from '@mysten/sui.js/cryptography';
import { Ed25519PublicKey } from '@mysten/sui.js/keypairs/ed25519';
import { WalletSigner } from './WalletSigner';

import type SuiLedgerClient from '@mysten/ledgerjs-hw-app-sui';

export class LedgerSigner extends WalletSigner {
	#suiLedgerClient: SuiLedgerClient | null;
	readonly #connectToLedger: () => Promise<SuiLedgerClient>;
	readonly #derivationPath: string;
	readonly #signatureScheme: SignatureScheme = 'ED25519';

	constructor(
		connectToLedger: () => Promise<SuiLedgerClient>,
		derivationPath: string,
		client: SuiClient,
	) {
		super(client);
		this.#connectToLedger = connectToLedger;
		this.#suiLedgerClient = null;
		this.#derivationPath = derivationPath;
	}

	async #initializeSuiLedgerClient() {
		if (!this.#suiLedgerClient) {
			// We want to make sure that there's only one connection established per Ledger signer
			// instance since some methods make multiple calls like getAddress and signData
			this.#suiLedgerClient = await this.#connectToLedger();
		}
		return this.#suiLedgerClient;
	}

	async getAddress(): Promise<string> {
		const ledgerClient = await this.#initializeSuiLedgerClient();
		const publicKeyResult = await ledgerClient.getPublicKey(this.#derivationPath);
		const publicKey = new Ed25519PublicKey(publicKeyResult.publicKey);
		return publicKey.toSuiAddress();
	}

	async getPublicKey(): Promise<Ed25519PublicKey> {
		const ledgerClient = await this.#initializeSuiLedgerClient();
		const { publicKey } = await ledgerClient.getPublicKey(this.#derivationPath);
		return new Ed25519PublicKey(publicKey);
	}

	async signData(data: Uint8Array): Promise<SerializedSignature> {
		const ledgerClient = await this.#initializeSuiLedgerClient();
		const { signature } = await ledgerClient.signTransaction(this.#derivationPath, data);
		const pubKey = await this.getPublicKey();
		return toSerializedSignature({
			signature,
			signatureScheme: this.#signatureScheme,
			pubKey,
		});
	}

	connect(client: SuiClient) {
		return new LedgerSigner(this.#connectToLedger, this.#derivationPath, client);
	}
}
