// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiGrpcClient } from '@mysten/sui/grpc';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { Transaction } from '@mysten/sui/transactions';

const client = new SuiGrpcClient({
    baseUrl: 'https://fullnode.mainnet.sui.io:443',
    network: 'mainnet',
});

// docs::#resolve-name
// Forward resolution is not part of the Core API, so use the gRPC name service
// directly. Native gRPC calls return the payload under `response`.
async function resolveRecipient(name: string): Promise<string> {
    const { response } = await client.nameService.lookupName({ name });
    const address = response.record?.targetAddress;

    if (!address) {
        throw new Error(`No address found for name: ${name}`);
    }

    return address;
}
// docs::/#resolve-name

// docs::#reverse-resolve
// Reverse resolution is available on the Core API, so it works on any client.
async function reverseResolve(address: string): Promise<string | null> {
    const { name } = await client.core.defaultNameServiceName({ address });
    return name ?? null;
}
// docs::/#reverse-resolve

// docs::#pay-by-name
async function payByName(name: string, amountMist: bigint, keypair: Ed25519Keypair) {
    const recipient = await resolveRecipient(name);

    const tx = new Transaction();
    const [coin] = tx.splitCoins(tx.gas, [amountMist]);
    tx.transferObjects([coin], recipient);

    const result = await client.core.signAndExecuteTransaction({
        transaction: tx,
        signer: keypair,
        include: { effects: true },
    });

    if (result.$kind === 'FailedTransaction') {
        throw new Error(`Payment to ${name} failed: ${result.FailedTransaction.status.error}`);
    }

    return result.Transaction.digest;
}
// docs::/#pay-by-name

export { payByName, resolveRecipient, reverseResolve };
