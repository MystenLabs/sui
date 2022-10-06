// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import axios from 'axios';

// NOTE: We import out of the source directory here to work around an issue with Cypress not
// respecting tsconfig paths in the config file.
import {
    Ed25519Keypair,
    JsonRpcProvider,
    RawSigner,
    type Keypair,
} from '../../../sdk/typescript/src';

export async function createLocalnetTasks() {
    const addressToKeypair = new Map<string, Keypair>();

    return {
        async mint(address: string) {
            const keypair = addressToKeypair.get(address);
            if (!keypair) {
                throw new Error('missing keypair');
            }
            const provider = new JsonRpcProvider('http://localhost:5001');
            const signer = new RawSigner(keypair, provider);

            const tx = await signer.executeMoveCall({
                packageObjectId: '0x2',
                module: 'devnet_nft',
                function: 'mint',
                typeArguments: [],
                arguments: [
                    'Example NFT',
                    'An example NFT.',
                    'ipfs://bafkreibngqhl3gaa7daob4i2vccziay2jjlp435cf66vhono7nrvww53ty',
                ],
                gasBudget: 30000,
            });

            return tx;
        },

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
