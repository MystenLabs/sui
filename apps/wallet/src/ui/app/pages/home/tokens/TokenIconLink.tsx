// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin } from '@mysten/core';
import { WalletActionStake24 } from '@mysten/icons';
import { SUI_TYPE_ARG } from '@mysten/sui.js';
import { useMemo } from 'react';

import { LargeButton } from '_app/shared/LargeButton';
import { DelegatedAPY } from '_app/shared/delegated-apy';
import { useGetDelegatedStake } from '_app/staking/useGetDelegatedStake';
import { ampli } from '_src/shared/analytics/ampli';

export function TokenIconLink({ accountAddress }: { accountAddress: string }) {
	const { data: delegatedStake, isLoading } = useGetDelegatedStake(accountAddress);

	// Total active stake for all delegations
	const totalActivePendingStake = useMemo(() => {
		if (!delegatedStake) return 0n;

		return delegatedStake.reduce(
			(acc, curr) => curr.stakes.reduce((total, { principal }) => total + BigInt(principal), acc),

			0n,
		);
	}, [delegatedStake]);

	const stakedValidators = delegatedStake?.map(({ validatorAddress }) => validatorAddress) || [];

	const [formatted, symbol, queryResult] = useFormatCoin(totalActivePendingStake, SUI_TYPE_ARG);

	return (
		<LargeButton
			to="/stake"
			onClick={() => {
				ampli.clickedStakeSui({
					isCurrentlyStaking: totalActivePendingStake > 0,
					sourceFlow: 'Home page',
				});
			}}
			loading={isLoading || queryResult.isLoading}
			before={<WalletActionStake24 />}
			center
			after={totalActivePendingStake ? <DelegatedAPY stakedValidators={stakedValidators} /> : null}
		>
			<div className="flex flex-col gap-1.25">
				<div>{totalActivePendingStake ? 'Currently Staked' : 'Stake and Earn SUI'}</div>
				{!!totalActivePendingStake && (
					<div>
						{formatted} {symbol}
					</div>
				)}
			</div>
		</LargeButton>
	);
}
