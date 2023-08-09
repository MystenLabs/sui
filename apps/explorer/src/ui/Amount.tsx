// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { CoinFormat, formatBalance } from '@mysten/core';
import { Heading } from '@mysten/ui';

export type AmountProps = {
	amount: number | string | bigint;
	symbol?: string | null;
	size?: 'md' | 'lg';
	format?: CoinFormat;
};

const DECIMALS = 0;

export function Amount({ amount, symbol, size = 'md', format }: AmountProps) {
	const isLarge = size === 'lg';

	// TODO: Remove this use-case, we should just enforce usage of this component in a specific way.
	// Instance where getCoinDenominationInfo is not available or amount component is used directly without useFormatCoin hook
	const formattedAmount =
		!symbol || typeof amount === 'bigint' || typeof amount === 'number'
			? formatBalance(amount, DECIMALS, format ?? CoinFormat.FULL)
			: amount;

	return (
		<div className="flex items-end gap-1 break-words">
			<Heading variant={isLarge ? 'heading2/bold' : 'heading6/semibold'} color="gray-90">
				{formattedAmount}
			</Heading>
			{symbol && (
				<div className="text-bodySmall font-medium leading-4 text-steel-dark">
					{isLarge ? <sup className="text-bodySmall">{symbol}</sup> : symbol}
				</div>
			)}
		</div>
	);
}
