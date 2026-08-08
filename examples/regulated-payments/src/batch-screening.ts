// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui/transactions';
import { SuiGrpcClient } from '@mysten/sui/grpc';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';

const client = new SuiGrpcClient({
	baseUrl: 'https://fullnode.mainnet.sui.io:443',
	network: 'mainnet',
});
const adminKeypair = Ed25519Keypair.fromSecretKey(process.env.ADMIN_SECRET_KEY!);
const denyCapId = '0xDENY_CAP_ID';

// docs::#batch-screening
const tx = new Transaction();

const addressesToFreeze = ['0xAddr1...', '0xAddr2...', '0xAddr3...'];

for (const addr of addressesToFreeze) {
	tx.moveCall({
		target: '0x2::coin::deny_list_v2_add',
		typeArguments: ['0xPACKAGE::module::MY_COIN'],
		arguments: [
			tx.object('0x403'), // DenyList shared object
			tx.object(denyCapId), // DenyCapV2
			tx.pure.address(addr), // Address to freeze
		],
	});
}

await client.signAndExecuteTransaction({ signer: adminKeypair, transaction: tx });
// docs::/#batch-screening
