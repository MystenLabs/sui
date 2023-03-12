// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    normalizeSuiAddress,
    type Keypair,
    type SuiAddress,
} from '@mysten/sui.js';

import { type Account, AccountType } from './Account';
import { AccountKeypair } from './AccountKeypair';

export type SerializedDerivedAccount = {
    type: AccountType.DERIVED;
    address: SuiAddress;
    derivationPath: string;
};

export class DerivedAccount implements Account {
    accountKeypair: AccountKeypair;
    type: AccountType;
    address: string;
    derivationPath: string;

    constructor({
        derivationPath,
        keypair,
    }: {
        derivationPath: string;
        keypair: Keypair;
    }) {
        this.type = AccountType.IMPORTED;
        this.derivationPath = derivationPath;
        this.accountKeypair = new AccountKeypair(keypair);
        this.address = normalizeSuiAddress(
            this.accountKeypair.publicKey.toSuiAddress()
        );
    }

    toJSON(): SerializedDerivedAccount {
        return {
            type: AccountType.DERIVED,
            address: this.address,
            derivationPath: this.derivationPath,
        };
    }
}
