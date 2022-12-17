// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
    isSuiObject,
    isSuiMoveObject,
} from '@mysten/sui.js';
import { useMemo } from 'react';
import { useParams, Navigate } from 'react-router-dom';

import ErrorResult from '~/components/error-result/ErrorResult';
import { useGetObject } from '~/hooks/useGetObject';
import { VALIDATORS_OBJECT_ID, type ValidatorState  } from '~/pages/validator/ValidatorDataTypes';
import { Heading } from '~/ui/Heading';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { getValidatorName } from '~/utils/getValidatorName';



function ValidatorDetails() {
    const { id } = useParams();
    
    const { data, isLoading, isSuccess, isError } =
        useGetObject(VALIDATORS_OBJECT_ID);

        const validatorsData =
        data && isSuiObject(data.details) && isSuiMoveObject(data.details.data)
            ? (data.details.data.fields as ValidatorState)
            : null;

        const validatorData = useMemo(() => {
            if (!validatorsData) return null;
            return validatorsData.validators.fields.active_validators.find(av => av.fields.metadata.fields.sui_address === id) || null;
        }, [id, validatorsData]);

        const validator = useMemo(() => {
            if (!validatorData) return null;

            const {name, pubkey_bytes, sui_address } = validatorData.fields.metadata.fields;

            return {
                name: getValidatorName(name),
                pubkey_bytes,
                sui_address,
            }

        }, [ validatorData]);

        
        if (!id) {
            return <Navigate to="/validators"  />;
         }

         

        if(!validator) {
            return (
                <ErrorResult
                    id={id}
                    errorMsg="No validator found with this address"
                    
                />
            );
        }

        

    return (
        <div className="mt-5 mb-10">
            {isLoading && (
                <LoadingSpinner />
            )}
            <div className="">
                <Heading as="h1" variant="heading2" weight="bold">Shinobi Systems 🚀 stakeview.app</Heading>
            </div>
        </div>
    )
    
}

export { ValidatorDetails };
