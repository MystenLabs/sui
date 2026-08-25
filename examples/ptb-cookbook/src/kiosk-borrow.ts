// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui/transactions';
import { SuiGrpcClient } from '@mysten/sui/grpc';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';

const client = new SuiGrpcClient({ baseUrl: 'https://fullnode.testnet.sui.io:443', network: 'testnet' });
const keypair = new Ed25519Keypair();

// docs::#kiosk-borrow
const tx = new Transaction();

const itemType = '0xPACKAGE::module::MyItem';
const kioskId = '0xKioskObjectId';
const kioskCapId = '0xKioskOwnerCapId';
const itemId = '0xItemObjectId';

// Borrow the item. Returns [item, borrowHotPotato].
const [item, borrow] = tx.moveCall({
	target: '0x2::kiosk::borrow_val',
	typeArguments: [itemType],
	arguments: [tx.object(kioskId), tx.object(kioskCapId), tx.pure('address', itemId)],
});

// Mutate the borrowed item (call your custom function).
tx.moveCall({
	target: '0xPACKAGE::module::enhance_item',
	arguments: [item],
});

// Return the item to the kiosk (consumes the hot potato).
tx.moveCall({
	target: '0x2::kiosk::return_val',
	typeArguments: [itemType],
	arguments: [tx.object(kioskId), item, borrow],
});

await client.signAndExecuteTransaction({ signer: keypair, transaction: tx });
// docs::/#kiosk-borrow
