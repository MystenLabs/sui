// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { normalizeSuiAddress, type SuiAddress } from '@mysten/sui.js';

import { type Account, AccountType } from './Account';
import { type Wallet } from '_src/shared/qredo-api';

export type SerializedQredoAccount = {
    type: AccountType.QREDO;
    address: SuiAddress;
    qredoConnectionID: string;
    qredoWalletID: string;
    labels?: Wallet['labels'];
    derivationPath: null;
};

export class QredoAccount implements Account {
    readonly type = AccountType.QREDO;
    readonly address: SuiAddress;
    readonly qredoConnectionID: string;
    readonly qredoWalletID: string;
    readonly labels: Wallet['labels'];

    constructor({
        address,
        qredoConnectionID,
        qredoWalletID,
        labels = [],
    }: Omit<SerializedQredoAccount, 'type' | 'derivationPath'>) {
        this.address = normalizeSuiAddress(address);
        this.qredoConnectionID = qredoConnectionID;
        this.qredoWalletID = qredoWalletID;
        this.labels = labels;
    }

    toJSON(): SerializedQredoAccount {
        return {
            type: this.type,
            address: this.address,
            qredoConnectionID: this.qredoConnectionID,
            qredoWalletID: this.qredoWalletID,
            labels: this.labels,
            derivationPath: null,
        };
    }
}
