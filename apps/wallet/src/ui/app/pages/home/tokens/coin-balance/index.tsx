// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useIsWalletDefiEnabled } from '_app/hooks/useIsWalletDefiEnabled';
import { useAppSelector } from '_hooks';
import { API_ENV } from '_shared/api-env';
import { Heading } from '_src/ui/app/shared/heading';
import { Text } from '_src/ui/app/shared/text';
import { useBalanceInUSD, useFormatCoin } from '@mysten/core';
import { SUI_TYPE_ARG } from '@mysten/sui/utils';
import { useMemo } from 'react';

export type CoinProps = {
	type: string;
	amount: bigint;
};

function WalletBalanceUsd({ amount: walletBalance }: { amount: bigint }) {
	const isDefiWalletEnabled = useIsWalletDefiEnabled();
	const formattedWalletBalance = useBalanceInUSD(SUI_TYPE_ARG, walletBalance);

	const walletBalanceInUsd = useMemo(() => {
		if (!formattedWalletBalance) return null;

		return `~${formattedWalletBalance.toLocaleString('en', {
			style: 'currency',
			currency: 'USD',
		})} USD`;
	}, [formattedWalletBalance]);

	if (!walletBalanceInUsd) {
		return null;
	}

	return (
		<Text variant="caption" weight="medium" color={isDefiWalletEnabled ? 'hero-darkest' : 'steel'}>
			{walletBalanceInUsd}
		</Text>
	);
}

export function CoinBalance({ amount: walletBalance, type }: CoinProps) {
	const { apiEnv } = useAppSelector((state) => state.app);
	const [formatted, symbol] = useFormatCoin(walletBalance, type);

	return (
		<div className="flex flex-col gap-1 items-center justify-center">
			<div className="flex items-center justify-center gap-2">
				<Heading leading="none" variant="heading1" weight="bold" color="gray-90">
					{formatted}
				</Heading>

				<Heading variant="heading6" weight="medium" color="steel">
					{symbol}
				</Heading>
			</div>
			<div>{apiEnv === API_ENV.mainnet ? <WalletBalanceUsd amount={walletBalance} /> : null}</div>
		</div>
	);
}
