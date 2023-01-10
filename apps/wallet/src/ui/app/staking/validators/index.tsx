// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import { useNavigate } from 'react-router-dom';

import { usePendingDelegation } from '../usePendingDelegation';
import { SelectValidatorCard } from './SelectValidatorCard';
import { ValidatorsCard } from './ValidatorsCard';
import { activeDelegationIDsSelector } from '_app/staking/selectors';
import Alert from '_components/alert';
import { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import Overlay from '_components/overlay';
import { useAppSelector, useObjectsState } from '_hooks';

export function Validators() {
    const [showModal, setShowModal] = useState(true);
    const [pendingDelegations, { isLoading: pendingDelegationsLoading }] =
        usePendingDelegation();
    const navigate = useNavigate();
    const close = () => {
        navigate('/');
    };

    const { loading, error } = useObjectsState();
    const activeDelegationIDs = useAppSelector(activeDelegationIDsSelector);

    const hasDelegations =
        activeDelegationIDs.length > 0 || pendingDelegations.length > 0;

    const pageTitle = hasDelegations
        ? 'Stake & Earn SUI'
        : 'Select a Validator';

    return (
        <Overlay
            showModal={showModal}
            setShowModal={setShowModal}
            title={pendingDelegationsLoading ? 'Loading' : pageTitle}
            closeIcon={SuiIcons.Close}
            closeOverlay={close}
        >
            <div className="w-full flex flex-col flex-nowrap h-full overflow-x-scroll">
                <Loading
                    loading={loading}
                    className="flex justify-center w-full items-center h-full"
                >
                    {error ? (
                        <Alert className="mb-2">
                            <strong>
                                Sync error (data might be outdated).
                            </strong>{' '}
                            <small>{error.message}</small>
                        </Alert>
                    ) : null}

                    {hasDelegations ? (
                        <ValidatorsCard />
                    ) : (
                        <SelectValidatorCard />
                    )}
                </Loading>
            </div>
        </Overlay>
    );
}
