// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// eslint-disable-next-line import/order
import 'tsconfig-paths/register';
// eslint-disable-next-line import/order
import {
    Ed25519Keypair,
    JsonRpcProvider,
    RawSigner,
    LocalTxnDataSerializer,
    type Keypair,
    assert,
    localnetConnection,
    SuiTransactionResponse,
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
    const signer = new RawSigner(
        keypair,
        provider,
        new LocalTxnDataSerializer(provider)
    );

    const [gasPayment] = await provider.getGasObjectsOwnedByAddress(
        keypair.getPublicKey().toSuiAddress()
    );

    const tx = await signer.signAndExecuteTransaction({
        kind: 'moveCall',
        data: {
            packageObjectId: '0x2',
            module: 'devnet_nft',
            function: 'mint',
            typeArguments: [],
            arguments: [
                'Example NFT',
                'An example NFT.',
                'ipfs://bafkreibngqhl3gaa7daob4i2vccziay2jjlp435cf66vhono7nrvww53ty',
            ],
            gasPayment: gasPayment.objectId,
            gasBudget: 30000,
        },
    });

    assert(tx, SuiTransactionResponse);
    return tx;
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
