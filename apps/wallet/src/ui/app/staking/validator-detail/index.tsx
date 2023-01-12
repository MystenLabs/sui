// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import { useSearchParams, useNavigate, Navigate } from 'react-router-dom';

import { ValidatorLogo } from '../validator-detail/ValidatorLogo';
import { ValidatorDetailCard } from './ValidatorDetailCard';
import { SuiIcons } from '_components/icon';
import Overlay from '_components/overlay';

export function ValidatorDetail() {
    const [searchParams] = useSearchParams();
    const validatorAddressParams = searchParams.get('address');
    const [showModal, setShowModal] = useState(true);

    const navigate = useNavigate();
    const close = () => {
        navigate('/');
    };

    if (!validatorAddressParams) {
        return <Navigate to={'/stake'} replace={true} />;
    }

    return (
        <Overlay
            showModal={showModal}
            setShowModal={setShowModal}
            title={
                <div className="flex gap-2 items-center">
                    <ValidatorLogo
                        validatorAddress={validatorAddressParams}
                        isTitle
                        iconSize="sm"
                        size="body"
                    />
                </div>
            }
            closeIcon={SuiIcons.Close}
            closeOverlay={close}
        >
            <ValidatorDetailCard validatorAddress={validatorAddressParams} />
        </Overlay>
    );
}
