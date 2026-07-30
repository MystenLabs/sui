// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui/transactions';
import { SuiGrpcClient } from '@mysten/sui/grpc';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';

const client = new SuiGrpcClient({ baseUrl: 'https://fullnode.testnet.sui.io:443', network: 'testnet' });
const userKeypair = new Ed25519Keypair();
const sponsorKeypair = new Ed25519Keypair();
const userAddress = userKeypair.toSuiAddress();
const sponsorAddress = sponsorKeypair.toSuiAddress();
const sponsorGasCoins = [{ objectId: '0x...', version: '1', digest: '...' }];

// docs::#sponsored
// === User side ===

// Build the transaction kind (no gas info yet).
const tx = new Transaction();
tx.moveCall({
	target: '0xPACKAGE::module::do_something',
	arguments: [tx.object('0xSomeObject')],
});

// Serialize only the transaction kind bytes for the sponsor.
const kindBytes = await tx.build({ client, onlyTransactionKind: true });

// === Sponsor side ===

// Reconstruct the transaction from the kind bytes.
const sponsoredTx = Transaction.fromKind(kindBytes);

// Set the user as the sender, sponsor as the gas owner.
sponsoredTx.setSender(userAddress);
sponsoredTx.setGasOwner(sponsorAddress);
sponsoredTx.setGasPayment(sponsorGasCoins); // Sponsor's coin objects

// Build the full transaction bytes.
const txBytes = await sponsoredTx.build({ client });

// === Both sign ===

// The sponsor signs first.
const sponsorSig = await sponsorKeypair.signTransaction(txBytes);
// The user signs second.
const userSig = await userKeypair.signTransaction(txBytes);

// === Submit ===

const result = await client.executeTransaction({
	transaction: txBytes,
	signatures: [userSig.signature, sponsorSig.signature],
});
// docs::/#sponsored

// docs::#sponsored-address-balance-gas
const sponsoredTx2 = Transaction.fromKind(kindBytes);
sponsoredTx2.setSender(userAddress);
sponsoredTx2.setGasOwner(sponsorAddress);

// Empty gas payment array tells the protocol to deduct gas from the
// sponsor's SUI address balance. The SDK sets the ValidDuring expiration
// and nonce automatically when you build with a connected client.
sponsoredTx2.setGasPayment([]);

// Building with a connected client resolves the expiration and nonce.
const txBytes2 = await sponsoredTx2.build({ client });
// docs::/#sponsored-address-balance-gas
