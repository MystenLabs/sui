// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import { TransactionBlock } from '@mysten/sui.js/transactions';
import { provider } from './rpc';
import { getFaucetHost, requestSuiFromFaucetV0 } from '@mysten/sui.js/src/faucet';

// This simulates what a server would do to sponsor a transaction
export async function sponsorTransaction(sender: string, transactionKindBytes: Uint8Array) {
	// Rather than do gas pool management, we just spin out a new keypair to sponsor the transaction with:
	const keypair = new Ed25519Keypair();
	const address = keypair.getPublicKey().toSuiAddress();
	await requestSuiFromFaucetV0({ recipient: address, host: getFaucetHost('testnet') });
	console.log(`Sponsor address: ${address}`);

	const tx = TransactionBlock.fromKind(transactionKindBytes);
	tx.setSender(sender);
	tx.setGasOwner(address);
	return await provider.signAndExecuteTransactionBlock({ signer: keypair, transactionBlock: tx });
}
