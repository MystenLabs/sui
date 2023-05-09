// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    normalizeSuiAddress,
    type SuiAddress,
    type HardenedEd25519Path,
} from '@mysten/sui.js';

import { type Account, AccountType } from './Account';

export type SerializedLedgerAccount = {
    type: AccountType.LEDGER;
    address: SuiAddress;
    derivationPath: HardenedEd25519Path;
};

export class LedgerAccount implements Account {
    readonly type: AccountType;
    readonly address: SuiAddress;
    readonly derivationPath: HardenedEd25519Path;

    constructor({
        address,
        derivationPath,
    }: {
        address: SuiAddress;
        derivationPath: HardenedEd25519Path;
    }) {
        this.type = AccountType.LEDGER;
        this.address = normalizeSuiAddress(address);
        this.derivationPath = derivationPath;
    }

    toJSON(): SerializedLedgerAccount {
        return {
            type: AccountType.LEDGER,
            address: this.address,
            derivationPath: this.derivationPath,
        };
    }
}
