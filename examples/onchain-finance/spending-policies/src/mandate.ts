// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui/transactions';
import type { SuiClient } from '@mysten/sui/client';
import type { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';

declare const client: SuiClient;
declare const ownerKeypair: Ed25519Keypair;
declare const agentKeypair: Ed25519Keypair;
declare const agentAddress: string;
declare const recipientA: string;
declare const recipientB: string;
declare const mandateObjectId: string;
declare const mandateOwnerCapId: string;
declare const recipient: string;
declare const amount: bigint;

const PACKAGE_ID = '0xYOUR_PACKAGE';

// docs::#create-mandate
const ONE_DAY_MS = 86_400_000;

const createTx = new Transaction();

// Create a mandate: max 10 SUI per tx, 100 SUI total, expires in 24 hours
const [ownerCap] = createTx.moveCall({
	target: `${PACKAGE_ID}::spending_mandate::create_mandate`,
	arguments: [
		createTx.pure.address(agentAddress),
		createTx.pure.u64(10_000_000_000),               // max_per_tx: 10 SUI
		createTx.pure.u64(100_000_000_000),              // total_cap: 100 SUI
		createTx.pure.vector('address', [recipientA, recipientB]),
		createTx.pure.u64(Date.now() + ONE_DAY_MS),      // expires_at_ms
	],
});

// Transfer the MandateOwnerCap to the owner (required, it has no `drop`)
createTx.transferObjects([ownerCap], ownerKeypair.toSuiAddress());

const createResult = await client.signAndExecuteTransaction({
	transaction: createTx,
	signer: ownerKeypair,
});
// docs::/#create-mandate

// docs::#execute-spend
const spendTx = new Transaction();

// Split the payment coin from gas
const [paymentCoin] = spendTx.splitCoins(spendTx.gas, [amount]);

// Execute the spend through the mandate
spendTx.moveCall({
	target: `${PACKAGE_ID}::spending_mandate::execute_spend`,
	typeArguments: ['0x2::sui::SUI'],
	arguments: [
		spendTx.object(mandateObjectId),    // SpendingMandate
		paymentCoin,                        // Coin to send
		spendTx.pure.address(recipient),    // Must be in allowlist
		spendTx.object('0x6'),              // Clock
	],
});

const spendResult = await client.signAndExecuteTransaction({
	transaction: spendTx,
	signer: agentKeypair,
});
// docs::/#execute-spend

// docs::#revoke-mandate
const revokeTx = new Transaction();

revokeTx.moveCall({
	target: `${PACKAGE_ID}::spending_mandate::revoke_mandate`,
	arguments: [
		revokeTx.object(mandateOwnerCapId),  // MandateOwnerCap
		revokeTx.object(mandateObjectId),    // SpendingMandate (agent must sign)
	],
});
// docs::/#revoke-mandate

export {};
