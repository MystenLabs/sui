// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { is, SuiObject } from '@mysten/sui.js';
import { useMemo } from 'react';
import { useParams } from 'react-router-dom';

import ErrorResult from '~/components/error-result/ErrorResult';
import { useGetObject } from '~/hooks/useGetObject';
import {
    VALIDATORS_OBJECT_ID,
    type ValidatorState,
} from '~/pages/validator/ValidatorDataTypes';
import { ValidatorMeta } from '~/pages/validator/ValidatorMeta';
import { ValidatorStats } from '~/pages/validator/ValidatorStats';
import { LoadingSpinner } from '~/ui/LoadingSpinner';

function ValidatorDetails() {
    const { id } = useParams();
    const { data, isLoading } = useGetObject(VALIDATORS_OBJECT_ID);

    const validatorsData =
        data &&
        is(data.details, SuiObject) &&
        data.details.data.dataType === 'moveObject'
            ? (data.details.data.fields as ValidatorState)
            : null;

    const validatorData = useMemo(() => {
        if (!validatorsData) return null;
        return (
            validatorsData.validators.fields.active_validators.find(
                (av) => av.fields.metadata.fields.sui_address === id
            ) || null
        );
    }, [id, validatorsData]);

    if (isLoading) {
        return (
            <div className="mt-5 mb-10 flex items-center justify-center">
                <LoadingSpinner />
            </div>
        );
    }

    if (!validatorData || !validatorsData) {
        return (
            <div className="mt-5 mb-10 flex items-center justify-center">
                <ErrorResult id={id} errorMsg="No validator data found" />
            </div>
        );
    }

    return (
        <div className="mt-5 mb-10">
            <div className="flex flex-col flex-nowrap gap-5 md:flex-row md:gap-0">
                <ValidatorMeta validatorData={validatorData} />
            </div>
            <div className="mt-5 flex w-full md:mt-8">
                <ValidatorStats
                    validatorData={validatorData}
                    epoch={validatorsData.epoch}
                    totalValidatorStake={
                        validatorsData.validators.fields.total_validator_stake
                    }
                />
            </div>
        </div>
    );
}

export { ValidatorDetails };
