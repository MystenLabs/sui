// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { normalizeSuiAddress, type SuiAddress } from '@mysten/sui.js';

import { type Account, AccountType } from './Account';

export type SerializedLedgerAccount = {
    type: AccountType.LEDGER;
    address: SuiAddress;
    derivationPath: string;
    publickey: string;
};

export class LedgerAccount implements Account {
    readonly type: AccountType;
    readonly address: SuiAddress;
    readonly derivationPath: string;
    readonly publickey: string;

    constructor({
        address,
        derivationPath,
        publickey,
    }: {
        address: SuiAddress;
        derivationPath: string;
        publickey: string;
    }) {
        this.type = AccountType.LEDGER;
        this.address = normalizeSuiAddress(address);
        this.derivationPath = derivationPath;
        this.publickey = publickey;
    }

    toJSON(): SerializedLedgerAccount {
        return {
            type: AccountType.LEDGER,
            address: this.address,
            derivationPath: this.derivationPath,
            publickey: this.publickey,
        };
    }
}
