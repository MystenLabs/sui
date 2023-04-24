// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    useGetRollingAverageApys,
    useGetValidatorsEvents,
    useGetSystemState,
} from '@mysten/core';
import { type SuiSystemStateSummary } from '@mysten/sui.js';
import React, { useMemo } from 'react';
import { useParams } from 'react-router-dom';

import { ValidatorMeta } from '~/components/validator/ValidatorMeta';
import { ValidatorStats } from '~/components/validator/ValidatorStats';
import { Banner } from '~/ui/Banner';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { Text } from '~/ui/Text';
import { getValidatorMoveEvent } from '~/utils/getValidatorMoveEvent';
import { VALIDATOR_LOW_STAKE_GRACE_PERIOD } from '~/utils/validatorConstants';

const getAtRiskRemainingEpochs = (
    data: SuiSystemStateSummary | undefined,
    validatorId: string | undefined
): number | null => {
    if (!data || !validatorId) return null;
    const atRisk = data.atRiskValidators.find(
        ([address]) => address === validatorId
    );
    return atRisk ? VALIDATOR_LOW_STAKE_GRACE_PERIOD - Number(atRisk[1]) : null;
};

function ValidatorDetails() {
    const { id } = useParams();
    const { data, isLoading } = useGetSystemState();

    const validatorData = useMemo(() => {
        if (!data) return null;
        return data.activeValidators.find((av) => av.suiAddress === id) || null;
    }, [id, data]);

    const atRiskRemainingEpochs = getAtRiskRemainingEpochs(data, id);

    const numberOfValidators = data?.activeValidators.length ?? null;
    const { data: rollingAverageApys, isLoading: validatorsApysLoading } =
        useGetRollingAverageApys(numberOfValidators);

    const { data: validatorEvents, isLoading: validatorsEventsLoading } =
        useGetValidatorsEvents({
            limit: numberOfValidators,
            order: 'descending',
        });

    const validatorRewards = useMemo(() => {
        if (!validatorEvents || !id) return 0;
        const rewards = getValidatorMoveEvent(
            validatorEvents,
            id
        )?.pool_staking_reward;

        return rewards ? Number(rewards) : null;
    }, [id, validatorEvents]);

    if (isLoading || validatorsEventsLoading || validatorsApysLoading) {
        return (
            <div className="mb-10 flex items-center justify-center">
                <LoadingSpinner />
            </div>
        );
    }

    if (!validatorData || !data || !validatorEvents || !id) {
        return (
            <div className="mb-10 flex items-center justify-center">
                <Banner variant="error" spacing="lg" fullWidth>
                    No validator data found for {id}
                </Banner>
            </div>
        );
    }

    const apy = rollingAverageApys?.[id] || 0;
    const tallyingScore =
        validatorEvents?.find(
            ({ parsedJson }) => parsedJson?.validator_address === id
        )?.parsedJson?.tallying_rule_global_score || null;
    return (
        <div className="mb-10">
            <div className="flex flex-col flex-nowrap gap-5 md:flex-row md:gap-0">
                <ValidatorMeta validatorData={validatorData} />
            </div>
            <div className="mt-5 md:mt-8">
                <ValidatorStats
                    validatorData={validatorData}
                    epoch={data.epoch}
                    epochRewards={validatorRewards}
                    apy={apy}
                    tallyingScore={tallyingScore}
                />
            </div>
            {atRiskRemainingEpochs !== null && (
                <div className="mt-5">
                    <Banner
                        fullWidth
                        border
                        variant="error"
                        title={
                            <Text uppercase variant="bodySmall/semibold">
                                at risk of being removed as a validator after{' '}
                                {atRiskRemainingEpochs} epoch
                                {atRiskRemainingEpochs > 1 ? 's' : ''}
                            </Text>
                        }
                    >
                        <Text variant="bodySmall/medium">
                            Staked SUI is below the minimum SUI stake threshold
                            to remain a validator.
                        </Text>
                    </Banner>
                </div>
            )}
        </div>
    );
}

export { ValidatorDetails };
