// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import { useNavigate } from 'react-router-dom';

import { useActiveAddress } from '../../hooks/useActiveAddress';
import { useGetDelegatedStake } from '../useGetDelegatedStake';
import { SelectValidatorCard } from './SelectValidatorCard';
import { ValidatorsCard } from './ValidatorsCard';
import Alert from '_components/alert';
import { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import Overlay from '_components/overlay';

export function Validators() {
    const [showModal, setShowModal] = useState(true);
    const accountAddress = useActiveAddress();
    const {
        data: stakedValidators,
        isLoading,
        isError,
        error,
    } = useGetDelegatedStake(accountAddress || '');

    const navigate = useNavigate();
    const close = () => {
        navigate('/');
    };

    const pageTitle = stakedValidators?.length
        ? 'Stake & Earn SUI'
        : 'Select a Validator';

    return (
        <Overlay
            showModal={showModal}
            setShowModal={setShowModal}
            title={isLoading ? 'Loading' : pageTitle}
            closeIcon={SuiIcons.Close}
            closeOverlay={close}
        >
            <div className="w-full flex flex-col flex-nowrap">
                <Loading loading={isLoading}>
                    {isError ? (
                        <Alert className="mb-2">
                            <strong>{error?.message}</strong>
                        </Alert>
                    ) : null}

                    {stakedValidators?.length ? (
                        <ValidatorsCard />
                    ) : (
                        <SelectValidatorCard />
                    )}
                </Loading>
            </div>
        </Overlay>
    );
}
