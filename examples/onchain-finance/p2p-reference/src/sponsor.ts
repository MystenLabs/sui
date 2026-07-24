// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui/transactions';
import { SuiClient } from '@mysten/sui/client';
import { toBase64, fromBase64 } from '@mysten/sui/utils';

declare const tx: Transaction;
declare const signer: { toSuiAddress(): string; signTransaction(bytes: Uint8Array): Promise<{ signature: string }> };
const client = new SuiClient({ url: 'https://fullnode.testnet.sui.io:443' });

// docs::#sponsor-flow
// Build transaction kind bytes (without gas)
const txKindBytes = await tx.build({ client, onlyTransactionKind: true });

// Request sponsorship
const sponsorResponse = await fetch('https://your-gas-station.com/sponsor', {
	method: 'POST',
	headers: { 'Content-Type': 'application/json' },
	body: JSON.stringify({ txBytes: toBase64(txKindBytes), sender: signer.toSuiAddress() }),
});

const { txBytes: sponsoredBytes, sponsorSignature, gasCoinId } = await sponsorResponse.json();

// Sign with the user's key
const finalBytes = fromBase64(sponsoredBytes);
const userSig = await signer.signTransaction(finalBytes);

// Submit with both signatures
const result = await client.executeTransactionBlock({
	transactionBlock: finalBytes,
	signature: [userSig.signature, sponsorSignature],
	options: { showEffects: true },
});

// Confirm to the gas station so it can release the coin.
// Always confirm, even on failure, because gas is still charged.
await fetch('https://your-gas-station.com/sponsor/confirm', {
	method: 'POST',
	headers: { 'Content-Type': 'application/json' },
	body: JSON.stringify({ gasCoinId, digest: result.digest }),
});
// docs::/#sponsor-flow

// docs::#gasless-submit
const gaslessResult = await client.signAndExecuteTransaction({
	transaction: tx,
	signer,
});
// docs::/#gasless-submit
