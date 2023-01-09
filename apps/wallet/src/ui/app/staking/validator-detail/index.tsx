// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { is, SuiObject } from '@mysten/sui.js';
import { useState, useMemo } from 'react';
import { useSearchParams, useNavigate, Navigate } from 'react-router-dom';

import { getName, STATE_OBJECT } from '../usePendingDelegation';
import { ValidatorDetailCard } from './ValidatorDetailCard';
import { ImageIcon } from '_app/shared/image-icon';
import Alert from '_components/alert';
import { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import Overlay from '_components/overlay';
import { useObjectsState, useGetObject } from '_hooks';

import type { ValidatorState } from '../ValidatorDataTypes';

export function ValidatorDetail() {
    const { loading, error, showError } = useObjectsState();
    const [searchParams] = useSearchParams();
    const validatorAddressParams = searchParams.get('address');
    const [showModal, setShowModal] = useState(true);

    const { data, isLoading } = useGetObject(STATE_OBJECT);

    const navigate = useNavigate();
    const close = () => {
        navigate('/');
    };

    const validatorsData =
        data &&
        is(data.details, SuiObject) &&
        data.details.data.dataType === 'moveObject'
            ? (data.details.data.fields as ValidatorState)
            : null;

    const validatorData = useMemo(() => {
        if (!validatorsData) return null;

        const validator =
            validatorsData.validators.fields.active_validators.find(
                (av) =>
                    av.fields.metadata.fields.sui_address ===
                    validatorAddressParams
            );

        if (!validator) return null;

        const { name: rawName } = validator.fields.metadata.fields;

        return {
            name: getName(rawName),
            logo: null,
        };
    }, [validatorAddressParams, validatorsData]);

    if (!validatorAddressParams) {
        return <Navigate to={'/stake'} replace={true} />;
    }

    const validatorName = validatorData?.name || 'Loading...';

    return (
        <Overlay
            showModal={showModal}
            setShowModal={setShowModal}
            title={
                <div className="flex gap-2 items-center capitalize">
                    <ImageIcon
                        src={validatorData?.logo}
                        alt={validatorName}
                        size="small"
                    />
                    {validatorName}
                </div>
            }
            closeIcon={SuiIcons.Close}
            closeOverlay={close}
        >
            <Loading
                className="w-full flex justify-center items-center"
                loading={loading || isLoading}
            >
                {showError && error && (
                    <Alert className="mb-2">
                        <strong>Sync error (data might be outdated).</strong>
                        <small>{error.message}</small>
                    </Alert>
                )}

                <ValidatorDetailCard
                    validatorAddress={validatorAddressParams}
                />
            </Loading>
        </Overlay>
    );
}
