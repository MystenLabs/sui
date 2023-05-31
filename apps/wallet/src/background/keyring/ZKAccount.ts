// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiAddress } from '@mysten/sui.js';

import { type StoredZkLoginAccount } from '../zk-login/storage';
import { type Account, AccountType } from './Account';

export type SerializedZKAccount = {
    type: AccountType.ZK;
    address: SuiAddress;
    sub: string;
    email: string;
    pin: string;
    derivationPath: null;
};

export class ZKAccount implements Account {
    readonly type: AccountType;
    readonly address: SuiAddress;
    readonly sub: string;
    readonly email: string;
    readonly pin: string;

    constructor({ address, sub, email, pin }: StoredZkLoginAccount) {
        this.type = AccountType.ZK;
        this.address = address;
        this.sub = sub;
        this.email = email;
        this.pin = pin;
    }

    toJSON(): SerializedZKAccount {
        return {
            type: AccountType.ZK,
            address: this.address,
            sub: this.sub,
            email: this.email,
            pin: this.pin,
            derivationPath: null,
        };
    }
}
