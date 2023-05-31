// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiAddress } from '@mysten/sui.js';

import { authenticateAccount } from '../zk-login';
import { getCachedAccountCredentials } from '../zk-login/keys-vault';
import { type StoredZkLoginAccount } from '../zk-login/storage';
import { type Account, AccountType } from './Account';
import { AccountKeypair } from './AccountKeypair';

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

    async signData(data: Uint8Array) {
        const credentials = await getCachedAccountCredentials(this.address);
        if (!credentials) {
            throw new Error('Account is locked');
        }
        const { ephemeralKeyPair, proofs } = credentials;
        const accountKeyPair = new AccountKeypair(ephemeralKeyPair);
        const signature = await accountKeyPair.sign(data);
        // TODO: create the actual zk signature
        return signature;
    }

    async ensureUnlocked(currentEpoch: number) {
        const credentials = await getCachedAccountCredentials(this.address);
        if (
            !credentials ||
            currentEpoch > credentials.proofs.aux_inputs.max_epoch
        ) {
            await authenticateAccount(this.address, currentEpoch);
        }
        return true;
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
