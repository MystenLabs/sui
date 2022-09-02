// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import axios from 'axios';

// NOTE: We import out of the source directory here to work around an issue with Cypress not
// respecting tsconfig paths in the config file.
import { Ed25519Keypair, type Keypair } from '../../../sdk/typescript/src';

export async function createLocalnetTasks() {
    const addressToKeypair = new Map<string, Keypair>();

    return {
        async faucet() {
            const keypair = Ed25519Keypair.generate();
            const address = keypair.getPublicKey().toSuiAddress();
            addressToKeypair.set(address, keypair);
            const res = await axios.post<{ ok: boolean }>(
                'http://127.0.0.1:9123/faucet',
                { recipient: address }
            );
            if (!res.data.ok) {
                throw new Error('Unable to invoke local faucet.');
            }
            return address;
        },
    };
}
