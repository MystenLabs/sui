// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { SuiClient } from '@mysten/sui/client';
import { Transaction } from '@mysten/sui/transactions';

// docs::#generate-new
const keypair = new Ed25519Keypair();
const address = keypair.toSuiAddress();

console.log('Agent address:', address);
console.log('Secret key:', keypair.getSecretKey()); // Bech32: suiprivkey1...
// docs::/#generate-new

// docs::#from-mnemonic
const mnemonic = 'word1 word2 word3 ... word12';
const mnemonicKeypair = Ed25519Keypair.deriveKeypair(mnemonic);
// docs::/#from-mnemonic

// docs::#from-secret-key
// fromSecretKey accepts the Bech32 suiprivkey... string directly
const envKeypair = Ed25519Keypair.fromSecretKey(process.env.AGENT_SECRET_KEY!);
// docs::/#from-secret-key

// docs::#env-load
const loadedKeypair = Ed25519Keypair.fromSecretKey(process.env.AGENT_SECRET_KEY!);
// docs::/#env-load

// docs::#derive-address
const agentAddress = keypair.toSuiAddress();
// docs::/#derive-address

const client = new SuiClient({ url: 'https://fullnode.testnet.sui.io:443' });

// docs::#sign-execute
const tx = new Transaction();
tx.setSender(keypair.toSuiAddress());
tx.moveCall({ target: '0xPACKAGE::module::function' });

// Sign and execute in one step
const result = await client.signAndExecuteTransaction({
	transaction: tx,
	signer: keypair,
});
// docs::/#sign-execute

// docs::#sign-sponsored
const bytes = await tx.build({ client });
const { signature } = await keypair.signTransaction(bytes);
// docs::/#sign-sponsored

// docs::#key-rotation
declare const oldAddress: string;
declare const newAddress: string;
declare const oldKeypair: Ed25519Keypair;

// Transfer all owned objects to the new address
const { data: ownedObjects } = await client.getOwnedObjects({
	owner: oldAddress,
});

const rotateTx = new Transaction();
rotateTx.setSender(oldAddress);
for (const item of ownedObjects) {
	if (!item.data) continue;
	rotateTx.transferObjects(
		[rotateTx.object(item.data.objectId)],
		newAddress,
	);
}

await client.signAndExecuteTransaction({ transaction: rotateTx, signer: oldKeypair });
// docs::/#key-rotation

export {};
