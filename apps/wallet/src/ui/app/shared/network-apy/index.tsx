// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';

import { calculateAPY } from '../../staking/calculateAPY';
import { STATE_OBJECT } from '../../staking/usePendingDelegation';
import { Text } from '_app/shared/text';
import { IconTooltip } from '_app/shared/tooltip';
import { validatorsFields } from '_app/staking/validatorsFields';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { roundFloat } from '_helpers';
import { useGetObject } from '_hooks';

const APY_DECIMALS = 3;

export function NetworkApy() {
    const { data, isLoading } = useGetObject(STATE_OBJECT);

    const validatorsData = data && validatorsFields(data);

    const averageNetworkAPY = useMemo(() => {
        if (!validatorsData) return 0;
        const validators = validatorsData.validators.fields.active_validators;

        const validatorsApy = validators.map((av) =>
            calculateAPY(av, +validatorsData.epoch)
        );
        return roundFloat(
            (validatorsApy.reduce((acc, cur) => acc + cur, 0) /
                validatorsApy.length,
            APY_DECIMALS)
        );
    }, [validatorsData]);

    if (isLoading) {
        return (
            <div className="p-2 w-full flex justify-center items-center h-full">
                <LoadingIndicator />
            </div>
        );
    }
    return (
        <div className="flex gap-0.5 items-center">
            {averageNetworkAPY && (
                <Text variant="body" weight="semibold" color="steel-dark">
                    {averageNetworkAPY}
                </Text>
            )}
            <Text variant="subtitle" weight="medium" color="steel-darker">
                {averageNetworkAPY > 0 ? `% APY` : '--'}
            </Text>

            <div className="text-steel items-baseline text-body flex">
                <IconTooltip tip="Annual Percentage Yield" placement="top" />
            </div>
        </div>
    );
}
