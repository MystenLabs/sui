// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    type SuiAddress,
    type HardwareKeypair,
    toSerializedSignature,
    type SerializedSignature,
} from '@mysten/sui.js';

import { type Account, AccountType } from './Account';

export type SerializedLedgerAccount = {
    type: AccountType.LEDGER;
    address: SuiAddress;
    derivationPath: string;
};

export class LedgerAccount implements Account {
    #keypair: HardwareKeypair;
    readonly type: AccountType;
    readonly address: SuiAddress;
    readonly derivationPath: string;

    constructor({
        address,
        derivationPath,
        keypair,
    }: {
        address: SuiAddress;
        derivationPath: string;
        keypair: HardwareKeypair;
    }) {
        this.type = AccountType.LEDGER;
        this.#keypair = keypair;
        this.address = address;
        this.derivationPath = derivationPath;
    }

    async sign(data: Uint8Array): Promise<SerializedSignature> {
        const pubkey = await this.#keypair.getPublicKey();
        const signature = await this.#keypair.signData(data);
        const signatureScheme = this.#keypair.getKeyScheme();
        return toSerializedSignature({
            signature,
            signatureScheme,
            pubKey: pubkey,
        });
    }

    toJSON(): SerializedLedgerAccount {
        return {
            type: AccountType.LEDGER,
            address: this.address,
            derivationPath: this.derivationPath,
        };
    }
}
