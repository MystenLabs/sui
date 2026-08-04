// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui/transactions';

declare const recipient: string;
declare const amount: bigint;
declare const registryId: string;
declare const nonce: string;

const PAYMENT_KIT_PACKAGE_ID = '0xPAYMENT_KIT';

// docs::#basic-transfer
const tx = new Transaction();

const [coin] = tx.splitCoins(tx.gas, [amount]);
tx.transferObjects([coin], recipient);
// docs::/#basic-transfer

// docs::#payment-kit
const pkTx = new Transaction();
const [paymentCoin] = pkTx.splitCoins(pkTx.gas, [amount]);

pkTx.moveCall({
    target: `${PAYMENT_KIT_PACKAGE_ID}::payment_kit::process_registry_payment`,
    typeArguments: ['0x2::sui::SUI'],
    arguments: [
        pkTx.object(registryId), // PaymentRegistry
        pkTx.pure.string(nonce), // Unique payment ID (UUIDv4)
        pkTx.pure.u64(amount), // Expected amount
        paymentCoin, // Coin to pay with
        pkTx.pure.option('address', recipient), // Receiver
        pkTx.object('0x6'), // Clock
    ],
});
// docs::/#payment-kit

export {};
