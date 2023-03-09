// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiAddress } from '@mysten/sui.js';
import { useMemo } from 'react';

import { calculateAPY } from '../../staking/calculateAPY';
import { useSystemState } from '../../staking/useSystemState';
import { Text } from '_app/shared/text';
import { IconTooltip } from '_app/shared/tooltip';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { roundFloat } from '_helpers';

const APY_DECIMALS = 3;

type DelegatedAPYProps = {
    stakedValidators: SuiAddress[];
};

export function DelegatedAPY({ stakedValidators }: DelegatedAPYProps) {
    const { data, isLoading } = useSystemState();

    const averageNetworkAPY = useMemo(() => {
        if (!data) return 0;
        const validators = data.active_validators;

        let stakedAPYs = 0;

        validators.forEach((validator) => {
            if (stakedValidators.includes(validator.sui_address)) {
                stakedAPYs += calculateAPY(validator, +data.epoch);
            }
        });

        const averageAPY = stakedAPYs / stakedValidators.length;

        return roundFloat(averageAPY || 0, APY_DECIMALS);
    }, [stakedValidators, data]);

    if (isLoading) {
        return (
            <div className="p-2 w-full flex justify-center items-center h-full">
                <LoadingIndicator />
            </div>
        );
    }
    return (
        <div className="flex gap-0.5 items-center">
            {averageNetworkAPY > 0 ? (
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
