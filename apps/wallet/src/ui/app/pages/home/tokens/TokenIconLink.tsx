// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { SUI_TYPE_ARG, type SuiAddress } from '@mysten/sui.js';
import { useMemo } from 'react';
import { Link } from 'react-router-dom';

import { NetworkApy } from '_app/shared/network-apy';
import { Text } from '_app/shared/text';
import { useGetDelegatedStake } from '_app/staking/useGetDelegatedStake';
import Icon from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { SuiIcons } from '_font-icons/output/sui-icons';
import { useFormatCoin } from '_hooks';
import { FEATURES } from '_src/shared/experimentation/features';

export function TokenIconLink({
    accountAddress,
}: {
    accountAddress: SuiAddress;
}) {
    const stakingEnabled = useFeature(FEATURES.STAKING_ENABLED).on;
    const { data: delegations, isLoading } =
        useGetDelegatedStake(accountAddress);

    const totalActivePendingStake = useMemo(() => {
        if (!delegations) return 0n;
        return delegations.reduce(
            (acc, { staked_sui }) => acc + BigInt(staked_sui.principal.value),
            0n
        );
    }, [delegations]);

    const [formatted, symbol, queryResult] = useFormatCoin(
        totalActivePendingStake,
        SUI_TYPE_ARG
    );

    return (
        <Link
            to="/stake"
            className="flex mb-5 rounded-2xl w-full p-5 justify-between no-underline bg-sui/10 text-hero"
            tabIndex={!stakingEnabled ? -1 : undefined}
        >
            {isLoading || queryResult.isLoading ? (
                <div className="p-2 w-full flex justify-center items-center h-full">
                    <LoadingIndicator />
                </div>
            ) : (
                <div className="flex gap-2.5 items-center">
                    <Icon
                        icon={SuiIcons.Union}
                        className="text-heading4 font-normal"
                    />
                    <div className="flex flex-col gap-1.25">
                        <Text variant="body" weight="semibold" color="hero">
                            {totalActivePendingStake
                                ? 'Currently Staked'
                                : 'Stake & Earn SUI'}
                        </Text>
                        {!!totalActivePendingStake && (
                            <Text variant="body" weight="semibold" color="hero">
                                {formatted} {symbol}
                            </Text>
                        )}
                    </div>
                </div>
            )}
            <div className="flex">{stakingEnabled && <NetworkApy />}</div>
        </Link>
    );
}
