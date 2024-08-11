// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { LargeButton } from '_app/shared/LargeButton';
import { ampli } from '_src/shared/analytics/ampli';
import {
	DELEGATED_STAKES_QUERY_REFETCH_INTERVAL,
	DELEGATED_STAKES_QUERY_STALE_TIME,
} from '_src/shared/constants';
import { Text } from '_src/ui/app/shared/text';
import { useFormatCoin, useGetDelegatedStake } from '@mysten/core';
import { WalletActionStake24 } from '@mysten/icons';
import { SUI_TYPE_ARG } from '@mysten/sui/utils';
import { useMemo } from 'react';

export function TokenIconLink({
	accountAddress,
	disabled,
}: {
	accountAddress: string;
	disabled: boolean;
}) {
	const { data: delegatedStake, isPending } = useGetDelegatedStake({
		address: accountAddress,
		staleTime: DELEGATED_STAKES_QUERY_STALE_TIME,
		refetchInterval: DELEGATED_STAKES_QUERY_REFETCH_INTERVAL,
	});

	// Total active stake for all delegations
	const totalActivePendingStake = useMemo(() => {
		if (!delegatedStake) return 0n;
		return delegatedStake.reduce(
			(acc, curr) => curr.stakes.reduce((total, { principal }) => total + BigInt(principal), acc),
			0n,
		);
	}, [delegatedStake]);

	const [formatted, symbol, queryResult] = useFormatCoin(totalActivePendingStake, SUI_TYPE_ARG);

	return (
		<LargeButton
			to="/stake"
			spacing="sm"
			center={!totalActivePendingStake}
			disabled={disabled}
			onClick={() => {
				ampli.clickedStakeSui({
					isCurrentlyStaking: totalActivePendingStake > 0,
					sourceFlow: 'Home page',
				});
			}}
			loading={isPending || queryResult.isPending}
			before={<WalletActionStake24 />}
			data-testid={`stake-button-${formatted}-${symbol}`}
		>
			<div className="flex flex-col">
				<Text variant="pBody" weight="semibold">
					{totalActivePendingStake ? 'Currently Staked' : 'Stake and Earn SUI'}
				</Text>

				{!!totalActivePendingStake && (
					<Text variant="pBody" weight="semibold">
						{formatted} {symbol}
					</Text>
				)}
			</div>
		</LargeButton>
	);
}
