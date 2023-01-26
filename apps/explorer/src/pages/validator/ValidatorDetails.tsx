// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { is, SuiObject, type ValidatorsFields } from '@mysten/sui.js';
import { useMemo } from 'react';
import { useParams } from 'react-router-dom';

import { ValidatorMeta } from '~/components/validator/ValidatorMeta';
import { ValidatorStats } from '~/components/validator/ValidatorStats';
import { useGetObject } from '~/hooks/useGetObject';
import { VALIDATORS_OBJECT_ID } from '~/pages/validator/ValidatorDataTypes';
import { Banner } from '~/ui/Banner';
import { LoadingSpinner } from '~/ui/LoadingSpinner';

function ValidatorDetails() {
    const { id } = useParams();
    const { data, isLoading } = useGetObject(VALIDATORS_OBJECT_ID);

    const validatorsData =
        data &&
        is(data.details, SuiObject) &&
        data.details.data.dataType === 'moveObject'
            ? (data.details.data.fields as ValidatorsFields)
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
                    totalValidatorStake={
                        validatorsData.validators.fields.total_validator_stake
                    }
                />
            </div>
        </div>
    );
}

export { ValidatorDetails };
