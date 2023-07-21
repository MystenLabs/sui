// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin, formatBalance, CoinFormat } from '@mysten/core';
import { SUI_TYPE_ARG } from '@mysten/sui.js';
import { Heading, Text } from '@mysten/ui';

type DelegationAmountProps = {
	amount: bigint | number | string;
	isStats?: boolean;
	inMIST?: boolean;
};

export function DelegationAmount({ amount, isStats, inMIST = false }: DelegationAmountProps) {
	const [formattedAmount, symbol] = useFormatCoin(amount, SUI_TYPE_ARG);
	const delegationAmount = inMIST ? formatBalance(amount, 0, CoinFormat.FULL) : formattedAmount;
	const delegationSymbol = inMIST ? 'MIST' : symbol;
	return isStats ? (
		<div className="flex items-end gap-1.5 break-all">
			<Heading as="div" variant="heading3/semibold" color="steel-darker">
				{delegationAmount}
			</Heading>
			<Heading variant="heading4/medium" color="steel-darker">
				{delegationSymbol}
			</Heading>
		</div>
	) : (
		<div className="flex h-full items-center gap-1">
			<div className="flex items-baseline gap-0.5 break-all text-steel-darker">
				<Text variant="body/medium">{delegationAmount}</Text>
				<Text variant="subtitleSmall/medium">{delegationSymbol}</Text>
			</div>
		</div>
	);
}
