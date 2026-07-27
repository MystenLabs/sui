// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiClient } from '@mysten/sui/client';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { Transaction } from '@mysten/sui/transactions';

const client = new SuiClient({ url: 'https://fullnode.mainnet.sui.io:443' });
const agentKeypair = Ed25519Keypair.fromSecretKey(process.env.AGENT_SECRET_KEY!);

// docs::#execute-recurring
async function executeRecurringPayment(
	mandateId: string,
	recipientAddress: string,
	amountMist: bigint,
) {
	const tx = new Transaction();
	tx.setSender(agentKeypair.toSuiAddress());

	tx.moveCall({
		target: '0xPACKAGE::spending_mandate::execute_spend',
		arguments: [
			tx.object(mandateId),
			tx.pure.u64(amountMist),
			tx.pure.address(recipientAddress),
			tx.object('0x6'), // Clock
		],
	});

	const result = await client.signAndExecuteTransaction({
		transaction: tx,
		signer: agentKeypair,
		options: { showEffects: true },
	});

	if (result.effects?.status.status !== 'success') {
		throw new Error(`Recurring payment failed: ${result.effects?.status.error}`);
	}

	return result.digest;
}
// docs::/#execute-recurring

export { executeRecurringPayment };
