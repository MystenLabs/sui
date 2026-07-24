// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction } from '@mysten/sui/transactions';
import type { DeepBookMarginClient } from './client.js';

// docs::#deposit-collateral
// Deposit collateral into the MarginManager. Use `depositBase` for the pool's
// base asset (SUI in SUI_DBUSDC) and `depositQuote` for the quote asset
// (DBUSDC). Collateral is what backs your borrow: the more you deposit before
// borrowing, the higher your starting risk ratio. The amount is in whole coins;
// the SDK scales it to the coin's decimals.
export function depositBaseCollateral(
	client: DeepBookMarginClient,
	managerKey: string,
	amount: number,
): Transaction {
	const tx = new Transaction();
	tx.add(client.deepbook.marginManager.depositBase({ managerKey, amount }));
	return tx;
}
// docs::/#deposit-collateral

// docs::#safe-collateral
// Never deposit more collateral than the wallet holds. `depositBase` sources
// coins with coinWithBalance, which throws at build time otherwise. Leave a SUI
// reserve so gas still has funds after the deposit.
export async function safeCollateralAmount(
	client: DeepBookMarginClient,
	owner: string,
	coinType: string,
	decimals: number,
	want: number,
	reserve = 0,
): Promise<number> {
	const b = await client.core.getBalance({ owner, coinType });
	const have = Number(b?.balance?.balance ?? '0') / 10 ** decimals;
	return Math.max(0, Math.min(want, have - reserve));
}
// docs::/#safe-collateral

// docs::#withdraw-collateral
// Withdraw collateral back to your wallet. A withdraw must leave your risk ratio
// at or above the pool's Min Withdraw Risk Ratio, so you cannot pull collateral
// out from under an open borrow: repay first, then withdraw. The amount is in
// whole coins.
export function withdrawBaseCollateral(
	client: DeepBookMarginClient,
	managerKey: string,
	amount: number,
): Transaction {
	const tx = new Transaction();
	tx.add(client.deepbook.marginManager.withdrawBase(managerKey, amount));
	return tx;
}
// docs::/#withdraw-collateral
