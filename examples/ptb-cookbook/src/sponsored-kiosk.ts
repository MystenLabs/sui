// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui/transactions';
import { SuiGrpcClient } from '@mysten/sui/grpc';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';

const client = new SuiGrpcClient({ baseUrl: 'https://fullnode.testnet.sui.io:443', network: 'testnet' });
const buyerKeypair = new Ed25519Keypair();
const sponsorKeypair = new Ed25519Keypair();
const buyerAddress = buyerKeypair.toSuiAddress();
const sponsorAddress = sponsorKeypair.toSuiAddress();
const sponsorGasCoins = [{ objectId: '0x...', version: '1', digest: '...' }];

// docs::#sponsored-kiosk
// === Buyer side ===

const tx = new Transaction();

const itemType = '0xPACKAGE::module::MyItem';
const kioskId = '0xSellerKioskId';
const itemId = '0xItemObjectId';
const price = 5_000_000_000n; // 5 SUI
const buyerKioskId = '0xBuyerKioskId';
const transferPolicyId = '0xTransferPolicyId';

// Withdraw from the buyer's address balance, then redeem to get a Coin<SUI>.
// Do NOT use tx.gas — after sponsorship, the gas coin belongs to the sponsor.
const [paymentCoin] = tx.moveCall({
	target: '0x2::coin::redeem_funds',
	typeArguments: ['0x2::sui::SUI'],
	arguments: [tx.withdrawal({ amount: price })],
});

// Purchase the item from the seller's kiosk.
const [item, transferRequest] = tx.moveCall({
	target: '0x2::kiosk::purchase',
	typeArguments: [itemType],
	arguments: [tx.object(kioskId), tx.pure('address', itemId), paymentCoin],
});

// Resolve the transfer policy (example: a rule that just needs confirmation).
tx.moveCall({
	target: '0xPOLICY_PACKAGE::my_rule::prove',
	typeArguments: [itemType],
	arguments: [tx.object(transferPolicyId), transferRequest],
});

// Confirm the transfer policy is satisfied.
tx.moveCall({
	target: '0x2::transfer_policy::confirm_request',
	typeArguments: [itemType],
	arguments: [tx.object(transferPolicyId), transferRequest],
});

// Place the item in the buyer's kiosk.
tx.moveCall({
	target: '0x2::kiosk::place',
	typeArguments: [itemType],
	arguments: [tx.object(buyerKioskId), tx.object('0xBuyerKioskCap'), item],
});

// Serialize kind bytes for the sponsor.
const kindBytes = await tx.build({ client, onlyTransactionKind: true });

// === Sponsor side ===

const sponsoredTx = Transaction.fromKind(kindBytes);
sponsoredTx.setSender(buyerAddress);
sponsoredTx.setGasOwner(sponsorAddress);
sponsoredTx.setGasPayment(sponsorGasCoins);

// Both sign and submit (same pattern as the sponsored transaction recipe).
// docs::/#sponsored-kiosk
