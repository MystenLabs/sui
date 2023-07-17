// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin, CoinFormat } from '@mysten/core';

import { Amount, type AmountProps } from '~/ui/Amount';

export interface CoinBalanceProps extends Omit<AmountProps, 'symbol'> {
	coinType?: string | null;
}

export function CoinBalance({ amount, coinType, format, ...props }: CoinBalanceProps) {
	const [formattedAmount, symbol] = useFormatCoin(amount, coinType, format || CoinFormat.FULL);

	// format balance if no symbol is provided
	// this handles instances where getCoinDenominationInfo is not available
	const formattedBalance = coinType ? formattedAmount : amount;

	return <Amount amount={formattedBalance} symbol={symbol} {...props} />;
}
