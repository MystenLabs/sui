// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSearchParams, useNavigate, Navigate } from 'react-router-dom';

import { ValidatorLogo } from '../validators/ValidatorLogo';
import { DelegationDetailCard } from './DelegationDetailCard';
import Overlay from '_components/overlay';

export function DelegationDetail() {
    const [searchParams] = useSearchParams();
    const validatorAddressParams = searchParams.get('validator');
    const stakeIdParams = searchParams.get('staked');
    const navigate = useNavigate();

    if (!validatorAddressParams || !stakeIdParams) {
        return <Navigate to={'/stake'} replace={true} />;
    }

    return (
        <Overlay
            showModal
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
            closeOverlay={() => navigate('/')}
        >
            <DelegationDetailCard
                validatorAddress={validatorAddressParams}
                stakedId={stakeIdParams}
            />
        </Overlay>
    );
}
