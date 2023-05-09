// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    type HardenedEd25519Path,
    type Secp256k1Path,
    type Keypair,
    type SuiAddress,
} from '@mysten/sui.js';

import { type Account, AccountType } from './Account';
import { AccountKeypair } from './AccountKeypair';

type DerivedAccountPath = HardenedEd25519Path | Secp256k1Path;

export type SerializedDerivedAccount = {
    type: AccountType.DERIVED;
    address: SuiAddress;
    derivationPath: DerivedAccountPath;
};

export class DerivedAccount implements Account {
    readonly accountKeypair: AccountKeypair;
    readonly type: AccountType;
    readonly address: SuiAddress;
    readonly derivationPath: DerivedAccountPath;

    constructor({
        derivationPath,
        keypair,
    }: {
        derivationPath: DerivedAccountPath;
        keypair: Keypair;
    }) {
        this.type = AccountType.IMPORTED;
        this.derivationPath = derivationPath;
        this.accountKeypair = new AccountKeypair(keypair);
        this.address = this.accountKeypair.publicKey.toSuiAddress();
    }

    toJSON(): SerializedDerivedAccount {
        return {
            type: AccountType.DERIVED,
            address: this.address,
            derivationPath: this.derivationPath,
        };
    }
}
