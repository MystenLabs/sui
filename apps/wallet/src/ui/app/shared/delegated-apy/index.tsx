// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    roundFloat,
    useGetRollingAverageApys,
    useGetSystemState,
} from '@mysten/core';
import { type SuiAddress } from '@mysten/sui.js';
import { useMemo } from 'react';

import { Text } from '_app/shared/text';
import { IconTooltip } from '_app/shared/tooltip';
import LoadingIndicator from '_components/loading/LoadingIndicator';

const APY_DECIMALS = 3;

type DelegatedAPYProps = {
    stakedValidators: SuiAddress[];
};

export function DelegatedAPY({ stakedValidators }: DelegatedAPYProps) {
    const { data, isLoading } = useGetSystemState();
    const { data: rollingAverageApys } = useGetRollingAverageApys(
        data?.activeValidators.length || null
    );

    const averageNetworkAPY = useMemo(() => {
        if (!data || !rollingAverageApys) return null;

        let stakedAPYs = 0;

        stakedValidators.forEach((validatorAddress) => {
            stakedAPYs += rollingAverageApys?.[validatorAddress] || 0;
        });

        const averageAPY = stakedAPYs / stakedValidators.length;

        return roundFloat(averageAPY || 0, APY_DECIMALS);
    }, [data, rollingAverageApys, stakedValidators]);

    if (isLoading) {
        return (
            <div className="p-2 w-full flex justify-center items-center h-full">
                <LoadingIndicator />
            </div>
        );
    }
    return (
        <div className="flex gap-0.5 items-center">
            {averageNetworkAPY !== null ? (
                <>
                    <Text variant="body" weight="semibold" color="steel-dark">
                        {averageNetworkAPY}
                    </Text>
                    <Text
                        variant="subtitle"
                        weight="medium"
                        color="steel-darker"
                    >
                        % APY
                    </Text>
                    <div className="text-steel items-baseline text-body flex">
                        <IconTooltip
                            tip="The average APY of all validators you are currently staking your SUI on."
                            placement="top"
                        />
                    </div>
                </>
            ) : (
                <Text variant="subtitle" weight="medium" color="steel-dark">
                    --
                </Text>
            )}
        </div>
    );
}
