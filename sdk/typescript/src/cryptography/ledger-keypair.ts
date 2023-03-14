// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0


import type SuiLedgerClient from '@mysten/ledgerjs-hw-app-sui';
import { SuiAddress } from '../types';
import { Ed25519PublicKey } from './ed25519-publickey';
import { HardwareKeypair, KeypairType } from './keypair';
import { SignatureScheme } from './signature';

export class LedgerKeypair implements HardwareKeypair {
    readonly type = KeypairType.HARDWARE
    readonly #suiLedgerClient: SuiLedgerClient;
    readonly #derivationPath: string;

    constructor(
        suiLedgerClient: SuiLedgerClient,
        derivationPath: string,
    ) {
        this.#suiLedgerClient = suiLedgerClient;
        this.#derivationPath = derivationPath;
    }

    getKeyScheme(): SignatureScheme {
        return 'ED25519';
    }

    async getAddress(): Promise<SuiAddress> {
        const publicKeyResult = await this.#suiLedgerClient.getPublicKey(
            this.#derivationPath
        );
        const publicKey = new Ed25519PublicKey(publicKeyResult.publicKey);
        return publicKey.toSuiAddress();
    }

    async getPublicKey(): Promise<Ed25519PublicKey> {
        const { publicKey } = await this.#suiLedgerClient.getPublicKey(
            this.#derivationPath
        );
        return new Ed25519PublicKey(publicKey);
    }

    async signData(data: Uint8Array): Promise<Uint8Array> {
        const { signature } = await this.#suiLedgerClient.signTransaction(
            this.#derivationPath,
            data
        );
        return signature;
    }
}
