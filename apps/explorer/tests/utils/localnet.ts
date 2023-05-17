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
    TransactionBlock,
} from '@mysten/sui.js';

const addressToKeypair = new Map<string, Keypair>();

export async function split_coin(address: string) {
    const keypair = addressToKeypair.get(address);
    if (!keypair) {
        throw new Error('missing keypair');
    }
    const provider = new JsonRpcProvider(localnetConnection);
    const signer = new RawSigner(keypair, provider);

    const coins = await provider.getCoins({ owner: address });
    const coin_id = coins.data[0].coinObjectId;

    const tx = new TransactionBlock();
    tx.moveCall({
        target: '0x2::pay::split',
        typeArguments: ['0x2::sui::SUI'],
        arguments: [tx.object(coin_id), tx.pure(10)],
    });

    const result = await signer.signAndExecuteTransactionBlock({
        transactionBlock: tx,
        options: {
            showInput: true,
            showEffects: true,
            showEvents: true,
        },
    });

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
