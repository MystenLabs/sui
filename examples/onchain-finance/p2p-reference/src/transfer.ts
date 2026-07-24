// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui/transactions';

declare const senderAddress: string;
declare const recipientAddress: string;

const USDC = '0xdba34672e30cb065b1f93e3ab55318768fd6fef66c15942c9f7cb846e2f900e7::usdc::USDC';

// docs::#build-transfer
const amount = 5_000_000n; // 5 USDC (6 decimals)

const tx = new Transaction();
tx.setSender(senderAddress);

tx.moveCall({
	target: '0x2::balance::send_funds',
	typeArguments: [USDC],
	arguments: [
		tx.balance({ type: USDC, balance: amount }),
		tx.pure.address(recipientAddress),
	],
});
// docs::/#build-transfer
