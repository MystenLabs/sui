// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useNavigate } from 'react-router-dom';

import { useActiveAddress } from '../../hooks/useActiveAddress';
import { useGetDelegatedStake } from '../useGetDelegatedStake';
import { SelectValidatorCard } from './SelectValidatorCard';
import { ValidatorsCard } from './ValidatorsCard';
import Alert from '_components/alert';
import Loading from '_components/loading';
import Overlay from '_components/overlay';

export function Validators() {
    const accountAddress = useActiveAddress();
    const {
        data: stakedValidators,
        isLoading,
        isError,
        error,
    } = useGetDelegatedStake(accountAddress || '');

    const navigate = useNavigate();

    const pageTitle = stakedValidators?.length
        ? 'Stake & Earn SUI'
        : 'Select a Validator';

    return (
        <Overlay
            showModal
            title={isLoading ? 'Loading' : pageTitle}
            closeOverlay={() => navigate('/')}
        >
            <div className="w-full flex flex-col flex-nowrap">
                <Loading loading={isLoading}>
                    {isError ? (
                        <div className="mb-2">
                            <Alert>
                                <strong>{error?.message}</strong>
                            </Alert>
                        </div>
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
