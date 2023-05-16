// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Keypair, type SuiAddress } from '@mysten/sui.js';

import { type Account, AccountType } from './Account';
import { AccountKeypair } from './AccountKeypair';

export type SerializedDerivedAccount = {
    type: AccountType.DERIVED;
    address: SuiAddress;
    derivationPath: string;
};

export class DerivedAccount implements Account {
    readonly accountKeypair: AccountKeypair;
    readonly type: AccountType;
    readonly address: SuiAddress;
    readonly derivationPath: string;

    constructor({
        derivationPath,
        keypair,
    }: {
        derivationPath: string;
        keypair: Keypair;
    }) {
        this.type = AccountType.DERIVED;
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
