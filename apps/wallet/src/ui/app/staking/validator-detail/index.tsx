// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import { useSearchParams, useNavigate, Navigate } from 'react-router-dom';

import { usePendingDelegation } from '../usePendingDelegation';
import { ValidatorDetailCard } from './ValidatorDetailCard';
import { ImageIcon } from '_app/shared/image-icon';
import Alert from '_components/alert';
import { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import Overlay from '_components/overlay';
import { useObjectsState } from '_hooks';

export function ValidatorDetail() {
    const { loading, error, showError } = useObjectsState();
    const [searchParams] = useSearchParams();
    const validatorAddressParams = searchParams.get('address');
    const [showModal, setShowModal] = useState(true);
    const [pendingDelegations, { isLoading: pendingDelegationsLoading }] =
        usePendingDelegation();

    const navigate = useNavigate();
    const close = () => {
        navigate('/');
    };

    if (!validatorAddressParams) {
        return <Navigate to={'/stake'} replace={true} />;
    }
    const validatorData = pendingDelegations.find(
        ({ validatorAddress }) => validatorAddress === validatorAddressParams
    );

    const validatorName = validatorData?.name || 'Loading...';

    // TODO: get logo from validator data
    const logo = null;

    return (
        <Overlay
            showModal={showModal}
            setShowModal={setShowModal}
            title={
                <div className="flex gap-2 items-center capitalize">
                    <ImageIcon src={logo} alt={validatorName} size="small" />
                    {validatorName}
                </div>
            }
            closeIcon={SuiIcons.Close}
            closeOverlay={close}
        >
            <Loading
                className="w-full flex justify-center items-center"
                loading={loading || pendingDelegationsLoading}
            >
                {showError && error && (
                    <Alert className="mb-2">
                        <strong>Sync error (data might be outdated).</strong>
                        <small>{error.message}</small>
                    </Alert>
                )}

                {validatorData && (
                    <ValidatorDetailCard
                        validatorAddress={validatorData.validatorAddress}
                        pendingDelegationAmount={validatorData.staked || 0n}
                        suiEarned={0n}
                        apy={'N/A'}
                        commissionRate={0}
                    />
                )}
            </Loading>
        </Overlay>
    );
}
