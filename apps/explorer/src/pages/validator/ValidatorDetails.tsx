// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';
import { useParams } from 'react-router-dom';

import { ValidatorMeta } from '~/components/validator/ValidatorMeta';
import { ValidatorStats } from '~/components/validator/ValidatorStats';
import { useGetSystemObject } from '~/hooks/useGetObject';
import { useGetValidatorsEvents } from '~/hooks/useGetValidatorsEvents';
import { Banner } from '~/ui/Banner';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { getValidatorMoveEvent } from '~/utils/getValidatorMoveEvent';

function ValidatorDetails() {
    const { id } = useParams();
    // TODO: Use `getValidators` once that API returns more data:
    const { data, isLoading } = useGetSystemObject();

    const validatorData = useMemo(() => {
        if (!data) return null;
        return (
            data.active_validators.find((av) => av.sui_address === id) || null
        );
    }, [id, data]);

    const numberOfValidators = data?.active_validators.length ?? null;

    const { data: validatorEvents, isLoading: validatorsEventsLoading } =
        useGetValidatorsEvents({
            limit: numberOfValidators,
            order: 'descending',
        });

    const validatorRewards = useMemo(() => {
        if (!validatorEvents || !id) return 0;
        return (
            getValidatorMoveEvent(validatorEvents.data, id)?.fields
                .stake_rewards || 0
        );
    }, [id, validatorEvents]);

    if (isLoading || validatorsEventsLoading) {
        return (
            <div className="mt-5 mb-10 flex items-center justify-center">
                <LoadingSpinner />
            </div>
        );
    }

    if (!validatorData || !data || !validatorEvents) {
        return (
            <div className="mt-5 mb-10 flex items-center justify-center">
                <Banner variant="error" spacing="lg" fullWidth>
                    No validator data found for {id}
                </Banner>
            </div>
        );
    }

    return (
        <div className="mt-5 mb-10">
            <div className="flex flex-col flex-nowrap gap-5 md:flex-row md:gap-0">
                <ValidatorMeta validatorData={validatorData} />
            </div>
            <div className="mt-5 md:mt-8">
                <ValidatorStats
                    validatorData={validatorData}
                    epoch={data.epoch}
                    epochRewards={validatorRewards}
                />
            </div>
        </div>
    );
}

export { ValidatorDetails };
