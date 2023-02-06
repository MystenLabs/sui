// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { is, SuiObject, type MoveSuiSystemObjectFields } from '@mysten/sui.js';
import { useMemo } from 'react';
import { useParams } from 'react-router-dom';

import { ValidatorMeta } from '~/components/validator/ValidatorMeta';
import { ValidatorStats } from '~/components/validator/ValidatorStats';
import { useGetObject } from '~/hooks/useGetObject';
import { useGetValidatorsEvents } from '~/hooks/useGetValidatorsEvents';
import { VALIDATORS_OBJECT_ID } from '~/pages/validator/ValidatorDataTypes';
import { Banner } from '~/ui/Banner';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { getValidatorMoveEvent } from '~/utils/getValidatorMoveEvent';

function ValidatorDetails() {
    const { id } = useParams();
    const { data, isLoading } = useGetObject(VALIDATORS_OBJECT_ID);

    const validatorsData =
        data &&
        is(data.details, SuiObject) &&
        data.details.data.dataType === 'moveObject'
            ? (data.details.data.fields as MoveSuiSystemObjectFields)
            : null;

    const validatorData = useMemo(() => {
        if (!validatorsData) return null;
        return (
            validatorsData.validators.fields.active_validators.find(
                (av) => av.fields.metadata.fields.sui_address === id
            ) || null
        );
    }, [id, validatorsData]);

    const numberOfValidators = useMemo(() => {
        return (
            validatorsData?.validators.fields.active_validators.length || null
        );
    }, [validatorsData]);

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

    if (!validatorData || !validatorsData || !validatorEvents) {
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
                    epoch={validatorsData.epoch}
                    epochRewards={validatorRewards}
                />
            </div>
        </div>
    );
}

export { ValidatorDetails };
