// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui/transactions';
import { SuiGrpcClient } from '@mysten/sui/grpc';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';

const client = new SuiGrpcClient({
	baseUrl: 'https://fullnode.mainnet.sui.io:443',
	network: 'mainnet',
});
const treasuryKeypair = Ed25519Keypair.fromSecretKey(process.env.TREASURY_SECRET_KEY!);
const coinType = '0x2::sui::SUI';
const refundAmount = 500_000_000n;
const userAddress = '0xUSER_ADDRESS';

// docs::#refund
const tx = new Transaction();

tx.moveCall({
	target: '0x2::balance::send_funds',
	typeArguments: [coinType],
	arguments: [tx.balance({ type: coinType, balance: refundAmount }), tx.pure.address(userAddress)],
});

await client.signAndExecuteTransaction({ signer: treasuryKeypair, transaction: tx });
// docs::/#refund
