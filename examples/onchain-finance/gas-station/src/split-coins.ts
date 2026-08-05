// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiGrpcClient } from '@mysten/sui/grpc';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { Transaction } from '@mysten/sui/transactions';

const client = new SuiGrpcClient({
    network: 'testnet',
    baseUrl: 'https://fullnode.testnet.sui.io:443',
});
const sponsor = Ed25519Keypair.fromSecretKey(process.env.SPONSOR_SECRET_KEY!);

// docs::#split-coins
const COIN_COUNT = 100;
const COIN_VALUE = 1_000_000_000n; // 1 SUI each

// Split the gas coin into many smaller coins so multiple transactions can be
// sponsored concurrently. The sponsor address must hold COIN_COUNT * COIN_VALUE
// plus gas before running this.
const tx = new Transaction();
const coins = tx.splitCoins(
    tx.gas,
    Array.from({ length: COIN_COUNT }, () => COIN_VALUE),
);
// One TransferObjects command for the whole batch, not one per coin.
tx.transferObjects(coins, sponsor.toSuiAddress());

const result = await client.signAndExecuteTransaction({ transaction: tx, signer: sponsor });
await client.waitForTransaction(result);
// docs::/#split-coins
