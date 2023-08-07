// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { useFormatCoin } from '@mysten/core';
import { WalletActionStake24 } from '@mysten/icons';
import { SUI_TYPE_ARG, type SuiAddress } from '@mysten/sui.js';
import { useMemo } from 'react';

import { LargeButton } from '_app/shared/LargeButton';
import { DelegatedAPY } from '_app/shared/delegated-apy';
import { useGetDelegatedStake } from '_app/staking/useGetDelegatedStake';
import { FEATURES } from '_src/shared/experimentation/features';
import { trackEvent } from '_src/shared/plausible';

export function TokenIconLink({
    accountAddress,
}: {
    accountAddress: SuiAddress;
}) {
    const stakingEnabled = useFeature(FEATURES.STAKING_ENABLED).on;
    const { data: delegatedStake, isLoading } =
        useGetDelegatedStake(accountAddress);

    // Total active stake for all delegations
    const totalActivePendingStake = useMemo(() => {
        if (!delegatedStake) return 0n;

        return delegatedStake.reduce(
            (acc, curr) =>
                curr.stakes.reduce(
                    (total, { principal }) => total + BigInt(principal),
                    acc
                ),

            0n
        );
    }, [delegatedStake]);

    const stakedValidators =
        delegatedStake?.map(({ validatorAddress }) => validatorAddress) || [];

    const [formatted, symbol, queryResult] = useFormatCoin(
        totalActivePendingStake,
        SUI_TYPE_ARG
    );

    return (
        <LargeButton
            to="/stake"
            onClick={() => {
                trackEvent('StakingFromHome');
            }}
            tabIndex={!stakingEnabled ? -1 : undefined}
            loading={isLoading || queryResult.isLoading}
            disabled={!stakingEnabled}
            before={<WalletActionStake24 />}
            after={
                stakingEnabled && (
                    <DelegatedAPY stakedValidators={stakedValidators} />
                )
            }
        >
            <div className="flex flex-col gap-1.25">
                <div>
                    {totalActivePendingStake
                        ? 'Currently Staked'
                        : 'Stake & Earn SUI'}
                </div>
                {!!totalActivePendingStake && (
                    <div>
                        {formatted} {symbol}
                    </div>
                )}
            </div>
        </LargeButton>
    );
}
