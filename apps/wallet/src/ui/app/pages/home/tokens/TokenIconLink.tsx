// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { SUI_TYPE_ARG, type SuiAddress } from '@mysten/sui.js';
import cl from 'classnames';
import { useMemo } from 'react';
import { Link } from 'react-router-dom';

import { DelegatedAPY } from '_app/shared/delegated-apy';
import { Text } from '_app/shared/text';
import { useGetDelegatedStake } from '_app/staking/useGetDelegatedStake';
import Icon from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { SuiIcons } from '_font-icons/output/sui-icons';
import { useFormatCoin } from '_hooks';
import { FEATURES } from '_src/shared/experimentation/features';
import { trackEvent } from '_src/shared/plausible';

export function TokenIconLink({
    accountAddress,
    noSuiToken,
}: {
    accountAddress: SuiAddress;
    noSuiToken?: boolean;
}) {
    const stakingEnabled =
        useFeature(FEATURES.STAKING_ENABLED).on && !noSuiToken;
    const { data: delegations, isLoading } =
        useGetDelegatedStake(accountAddress);

    const totalActivePendingStake = useMemo(() => {
        if (!delegations) return 0n;
        return delegations.reduce(
            (acc, { staked_sui }) => acc + BigInt(staked_sui.principal.value),
            0n
        );
    }, [delegations]);

    const stakedValidators = useMemo(() => {
        if (!delegations) return [];
        return delegations.map(
            ({ staked_sui }) => staked_sui.validator_address
        );
    }, [delegations]);

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
            className={cl(
                !stakingEnabled && '!bg-gray-40',
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
                    <Icon
                        icon={SuiIcons.Union}
                        className={cl(
                            !stakingEnabled ? 'text-gray-60' : 'text-hero',
                            'text-heading4 font-normal '
                        )}
                    />
                    <div className="flex flex-col gap-1.25">
                        <Text
                            variant="body"
                            weight="semibold"
                            color={!stakingEnabled ? 'gray-60' : 'hero'}
                        >
                            {totalActivePendingStake
                                ? 'Currently Staked'
                                : 'Stake & Earn SUI'}
                        </Text>
                        {!!totalActivePendingStake && (
                            <Text
                                variant="body"
                                weight="semibold"
                                color={!stakingEnabled ? 'gray-60' : 'hero'}
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
