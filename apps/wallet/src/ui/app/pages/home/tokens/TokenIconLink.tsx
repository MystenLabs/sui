// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { useFormatCoin } from '@mysten/core';
import { WalletActionStake24 } from '@mysten/icons';
import { SUI_TYPE_ARG, type SuiAddress } from '@mysten/sui.js';
import { cx } from 'class-variance-authority';
import { useMemo } from 'react';
import { Link } from 'react-router-dom';

import { DelegatedAPY } from '_app/shared/delegated-apy';
import { Text } from '_app/shared/text';
import { useGetDelegatedStake } from '_app/staking/useGetDelegatedStake';
import LoadingIndicator from '_components/loading/LoadingIndicator';
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
        <Link
            to="/stake"
            onClick={() => {
                trackEvent('StakingFromHome');
            }}
            className={cx(
                stakingEnabled ? '' : '!bg-gray-40',
                'flex rounded-2xl w-full p-3.75 justify-between no-underline bg-sui/10'
            )}
            tabIndex={!stakingEnabled ? -1 : undefined}
        >
            {isLoading || queryResult.isLoading ? (
                <div className="p-2 w-full flex justify-start items-center h-full">
                    <LoadingIndicator />
                </div>
            ) : (
                <div className="flex gap-2.5 items-center">
                    <WalletActionStake24
                        className={cx(
                            'text-heading2 bg-transparent',
                            stakingEnabled ? 'text-hero-dark ' : 'text-gray-60'
                        )}
                    />

                    <div className="flex flex-col gap-1.25">
                        <Text
                            variant="body"
                            weight="semibold"
                            color={stakingEnabled ? 'hero-dark' : 'gray-60'}
                        >
                            {totalActivePendingStake
                                ? 'Currently Staked'
                                : 'Stake & Earn SUI'}
                        </Text>
                        {!!totalActivePendingStake && (
                            <Text
                                variant="body"
                                weight="semibold"
                                color={stakingEnabled ? 'hero-dark' : 'gray-60'}
                            >
                                {formatted} {symbol}
                            </Text>
                        )}
                    </div>
                </div>
            )}
            <div className="flex">
                {stakingEnabled && (
                    <DelegatedAPY stakedValidators={stakedValidators} />
                )}
            </div>
        </Link>
    );
}
