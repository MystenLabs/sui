// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui/transactions';
import { SuiClient } from '@mysten/sui/client';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';

const client = new SuiClient({ url: 'https://fullnode.testnet.sui.io:443' });
const sponsor = new Ed25519Keypair(); // Your sponsor keypair

// docs::#split-coins
// Split gas coin into 100 coins of 1 SUI each
const tx = new Transaction();
const coins = tx.splitCoins(
	tx.gas,
	Array.from({ length: 100 }, () => 1_000_000_000n),
);
// Transfer all split coins back to sponsor (they become separate objects)
for (let i = 0; i < 100; i++) {
	tx.transferObjects([coins[i]], sponsor.toSuiAddress());
}

await client.signAndExecuteTransaction({ transaction: tx, signer: sponsor });
// docs::/#split-coins
