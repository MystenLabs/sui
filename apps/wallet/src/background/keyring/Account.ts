// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    normalizeSuiAddress,
    toSerializedSignature,
    type SerializedSignature,
    type Keypair,
    type SuiAddress,
} from '@mysten/sui.js';

import type { Keyring } from '.';
import type { AccountDetails } from '_payloads/wallet-status-change';

export type AccountType = 'derived' | 'imported';
export type AccountSerialized = {
    type: AccountType;
    address: SuiAddress;
    derivationPath: string | null;
};

export class Account {
    #keypair: Keypair;
    public readonly type: AccountType;
    public readonly derivationPath: string | null;
    public readonly address: SuiAddress;

    constructor(
        options:
            | { type: 'derived'; derivationPath: string; keypair: Keypair }
            | { type: 'imported'; keypair: Keypair }
    ) {
        this.type = options.type;
        this.derivationPath =
            options.type === 'derived' ? options.derivationPath : null;
        this.#keypair = options.keypair;
        this.address = normalizeSuiAddress(
            this.#keypair.getPublicKey().toSuiAddress()
        );
    }

    exportKeypair() {
        return this.#keypair.export();
    }

    async sign(data: Uint8Array): Promise<SerializedSignature> {
        const pubkey = this.#keypair.getPublicKey();
        // This is fine to hardcode useRecoverable = false because wallet does not support Secp256k1. Ed25519 does not use this parameter.
        const signature = this.#keypair.signData(data, false);
        const signatureScheme = this.#keypair.getKeyScheme();

        return toSerializedSignature({
            signature,
            signatureScheme,
            pubKey: pubkey,
        });
    }

    toJSON(): AccountSerialized {
        return {
            type: this.type,
            address: this.address,
            derivationPath: this.derivationPath,
        };
    }

    get publicKey() {
        return this.#keypair.getPublicKey();
    }
}

export async function getAccountsDetails(
    addresses: SuiAddress[],
    keyring: Keyring
) {
    const activeAccountAddress = (await keyring.getActiveAccount())?.address;
    const accounts = new Map<SuiAddress, AccountDetails>();
    for (const anAddress of addresses) {
        if (accounts.has(anAddress)) {
            continue;
        }
        const anAccount = keyring.getAccount(anAddress);
        if (anAccount) {
            accounts.set(anAddress, {
                address: anAddress,
                publicKey: anAccount.publicKey.toBase64(),
                label: '',
                selected: activeAccountAddress === anAddress,
            });
        }
    }
    return accounts;
}
