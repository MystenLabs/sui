// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin, useSuiCoinData } from '@mysten/core';
import { SUI_DECIMALS } from '@mysten/sui.js/utils';
import BigNumber from 'bignumber.js';
import { useMemo } from 'react';
import { CoinIcon } from '_src/ui/app/components/coin-icon';
import { Heading } from '_src/ui/app/shared/heading';
import { Text } from '_src/ui/app/shared/text';

export type CoinProps = {
	type: string;
	amount: bigint;
};

export function CoinBalance({ amount: walletBalance, type }: CoinProps) {
	const [formatted, symbol] = useFormatCoin(walletBalance, type);
	const { data } = useSuiCoinData();
	const { currentPrice } = data || {};

	const walletBalanceInUsd = useMemo(() => {
		if (!currentPrice) return null;
		const suiPriceInUsd = new BigNumber(currentPrice);
		const walletBalanceInSui = new BigNumber(walletBalance.toString()).shiftedBy(-1 * SUI_DECIMALS);
		const value = walletBalanceInSui.multipliedBy(suiPriceInUsd).toNumber();

		return `~${value.toLocaleString('en', {
			style: 'currency',
			currency: 'USD',
		})} USD`;
	}, [currentPrice, walletBalance]);

	return (
		<div className="flex flex-col gap-1 items-center justify-center">
			<div className="flex items-center justify-center gap-2">
				<CoinIcon fill="suiPrimary2023" coinType={type} size="sm" />
				<Heading leading="none" variant="heading1" color="gray-90">
					{formatted}
				</Heading>
				<div className="self-start mt-0.5">
					<Heading variant="heading6" weight="medium" color="steel">
						{symbol}
					</Heading>
				</div>
			</div>
			<div>
				{walletBalanceInUsd ? (
					<Text variant="caption" weight="medium" color="steel">
						{walletBalanceInUsd}
					</Text>
				) : null}
			</div>
		</div>
	);
}
