// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// eslint-disable-next-line import/order
import 'tsconfig-paths/register';
// eslint-disable-next-line import/order
import {
    Ed25519Keypair,
    JsonRpcProvider,
    RawSigner,
    type Keypair,
    localnetConnection,
    Transaction,
} from '@mysten/sui.js';

const addressToKeypair = new Map<string, Keypair>();

export async function mint(address: string) {
    const keypair = addressToKeypair.get(address);
    if (!keypair) {
        throw new Error('missing keypair');
    }
    const provider = new JsonRpcProvider(localnetConnection, {
        skipDataValidation: false,
    });
    const signer = new RawSigner(keypair, provider);

    const tx = new Transaction();
    tx.add(
        Transaction.Commands.MoveCall({
            target: '0x2::devnet_nft::mint',
            arguments: [
                tx.pure('Example NFT'),
                tx.pure('An example NFT.'),
                tx.pure(
                    'ipfs://bafkreibngqhl3gaa7daob4i2vccziay2jjlp435cf66vhono7nrvww53ty'
                ),
            ],
        })
    );
    tx.setGasBudget(30000);

    const result = await signer.signAndExecuteTransaction(tx);

    return result;
}

export async function faucet() {
    const keypair = Ed25519Keypair.generate();
    const address = keypair.getPublicKey().toSuiAddress();
    addressToKeypair.set(address, keypair);
    const res = await fetch('http://127.0.0.1:9123/gas', {
        method: 'POST',
        headers: {
            'content-type': 'application/json',
        },
        body: JSON.stringify({ FixedAmountRequest: { recipient: address } }),
    });
    const data = await res.json();
    if (!res.ok || data.error) {
        throw new Error('Unable to invoke local faucet.');
    }
    return address;
}
