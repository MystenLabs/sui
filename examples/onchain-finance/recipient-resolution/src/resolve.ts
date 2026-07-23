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
async function resolveRecipient(name: string): Promise<string> {
	const { response } = await client.nameService.lookupName({ name });
	const address = response.record?.targetAddress;

	if (!address) {
		throw new Error(`Name "${name}" not found or expired`);
	}

	return address;
}
// docs::/#resolve-name

// docs::#reverse-resolve
async function reverseResolve(address: string): Promise<string | null> {
	const result = await client.defaultNameServiceName({ address });
	return result.data.name;
}
// docs::/#reverse-resolve

// docs::#pay-by-name
async function payByName(
	name: string,
	amountMist: bigint,
	keypair: Ed25519Keypair,
) {
	const recipient = await resolveRecipient(name);

	const tx = new Transaction();
	const [coin] = tx.splitCoins(tx.gas, [amountMist]);
	tx.transferObjects([coin], recipient);

	return client.signAndExecuteTransaction({
		transaction: tx,
		signer: keypair,
	});
}
// docs::/#pay-by-name

export { resolveRecipient, reverseResolve, payByName };
