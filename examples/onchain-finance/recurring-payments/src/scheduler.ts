// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiGrpcClient } from '@mysten/sui/grpc';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { Transaction } from '@mysten/sui/transactions';

const client = new SuiGrpcClient({
    baseUrl: 'https://fullnode.mainnet.sui.io:443',
    network: 'mainnet',
});

const agentKeypair = Ed25519Keypair.fromSecretKey(process.env.AGENT_SECRET_KEY!);

const PACKAGE_ID = '0xPACKAGE';

// docs::#execute-recurring
async function executeRecurringPayment(
    mandateId: string,
    recipientAddress: string,
    amountMist: bigint,
) {
    const tx = new Transaction();
    tx.setSender(agentKeypair.toSuiAddress());

    // execute_spend takes the Coin to send, not an amount. Keep this call in sync
    // with the create/spend example in the spending-policies page.
    tx.moveCall({
        target: `${PACKAGE_ID}::spending_mandate::execute_spend`,
        typeArguments: ['0x2::sui::SUI'],
        arguments: [
            tx.object(mandateId), // SpendingMandate
            tx.coin({ balance: amountMist }), // Coin to send
            tx.pure.address(recipientAddress), // Must be in allowlist
            tx.object('0x6'), // Clock
        ],
    });

    const result = await client.signAndExecuteTransaction({
        transaction: tx,
        signer: agentKeypair,
        include: { effects: true },
    });

    if (result.$kind === 'FailedTransaction') {
        throw new Error(`Recurring payment failed: ${result.FailedTransaction.status.error}`);
    }

    return result.Transaction.digest;
}
// docs::/#execute-recurring

export { executeRecurringPayment };
