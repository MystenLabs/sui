// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui/transactions';
import { SuiClient } from '@mysten/sui/client';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { toBase64, fromBase64 } from '@mysten/sui/utils';

const client = new SuiClient({ url: 'https://fullnode.testnet.sui.io:443' });
const user = new Ed25519Keypair();

// docs::#client-flow
// 1. Build the transaction without gas
const tx = new Transaction();
tx.moveCall({ target: '0xPACKAGE::module::function' });

// 2. Serialize and send to gas station
const txBytes = await tx.build({ client, onlyTransactionKind: true });
const response = await fetch('http://localhost:3001/sponsor', {
	method: 'POST',
	headers: { 'Content-Type': 'application/json' },
	body: JSON.stringify({ txBytes: toBase64(txBytes), sender: user.toSuiAddress() }),
});

const { txBytes: sponsoredBytes, sponsorSignature, gasCoinId } = await response.json();

// 3. Sign with the user's key
const finalBytes = fromBase64(sponsoredBytes);
const userSig = await user.signTransaction(finalBytes);

// 4. Submit with both signatures
const result = await client.executeTransactionBlock({
	transactionBlock: finalBytes,
	signature: [userSig.signature, sponsorSignature],
	options: { showEffects: true },
});

// 5. Confirm to the gas station so it can release the coin.
//    Always confirm, even on failure, because gas is still charged.
await fetch('http://localhost:3001/sponsor/confirm', {
	method: 'POST',
	headers: { 'Content-Type': 'application/json' },
	body: JSON.stringify({ gasCoinId, digest: result.digest }),
});
// docs::/#client-flow
