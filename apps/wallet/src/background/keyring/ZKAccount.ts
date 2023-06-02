// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { BCS, fromB64, toB64 } from '@mysten/bcs';
import { type SuiAddress, bcs, SIGNATURE_SCHEME_TO_FLAG } from '@mysten/sui.js';

import { authenticateAccount } from '../zk-login';
import { getCachedAccountCredentials } from '../zk-login/keys-vault';
import { type StoredZkLoginAccount } from '../zk-login/storage';
import { type Account, AccountType } from './Account';
import { AccountKeypair } from './AccountKeypair';

bcs.registerStructType('ZKLoginSignature', {
    proof: {
        pi_a: [BCS.VECTOR, BCS.STRING],
        pi_b: [BCS.VECTOR, [BCS.VECTOR, BCS.STRING]],
        pi_c: [BCS.VECTOR, BCS.STRING],
        protocol: BCS.STRING,
    },
    public_inputs: {
        inputs: [BCS.VECTOR, BCS.STRING],
    },
    aux_inputs: {
        addr_seed: BCS.STRING,
        eph_public_key: [BCS.VECTOR, BCS.STRING],
        jwt_sha2_hash: [BCS.VECTOR, BCS.STRING],
        jwt_signature: BCS.STRING,
        key_claim_name: BCS.STRING,
        masked_content: [BCS.VECTOR, BCS.U8],
        max_epoch: BCS.U64,
        num_sha2_blocks: BCS.U8,
        payload_len: BCS.U16,
        payload_start_index: BCS.U16,
    },
    user_signature: [BCS.VECTOR, BCS.U8],
});

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
        const userSignature = await accountKeyPair.sign(data);
        const ZKLoginSignatureData = {
            proof: { ...proofs.proof_points, protocol: 'groth16' },
            public_inputs: { inputs: proofs.public_inputs },
            aux_inputs: proofs.aux_inputs,
            user_signature: fromB64(userSignature),
        };
        const bytes = bcs
            .ser('ZKLoginSignature', ZKLoginSignatureData)
            .toBytes();
        const signatureBytes = new Uint8Array(bytes.length + 1);
        signatureBytes.set([SIGNATURE_SCHEME_TO_FLAG['zkLoginFlag']]);
        signatureBytes.set(bytes, 1);
        return toB64(signatureBytes);
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
