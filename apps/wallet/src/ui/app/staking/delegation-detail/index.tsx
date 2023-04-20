// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSearchParams, useNavigate, Navigate } from 'react-router-dom';

import { useActiveAddress } from '../../hooks/useActiveAddress';
import { getDelegationDataByStakeId } from '../getDelegationByStakeId';
import { useGetDelegatedStake } from '../useGetDelegatedStake';
import { ValidatorLogo } from '../validators/ValidatorLogo';
import { DelegationDetailCard } from './DelegationDetailCard';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import Overlay from '_components/overlay';

export function DelegationDetail() {
    const [searchParams] = useSearchParams();
    const validatorAddressParams = searchParams.get('validator');
    const stakeIdParams = searchParams.get('staked');
    const navigate = useNavigate();
    const accountAddress = useActiveAddress();
    const { data, isLoading } = useGetDelegatedStake(accountAddress || '');

    if (!validatorAddressParams || !stakeIdParams) {
        return <Navigate to={'/stake'} replace={true} />;
    }

    if (isLoading) {
        return (
            <div className="p-2 w-full flex justify-center items-center h-full">
                <LoadingIndicator />
            </div>
        );
    }

    const delegationData = data
        ? getDelegationDataByStakeId(data, stakeIdParams)
        : null;
    return (
        <Overlay
            showModal
            title={
                <div className="flex items-center max-w-full px-4">
                    <ValidatorLogo
                        validatorAddress={validatorAddressParams}
                        isTitle
                        iconSize="sm"
                        size="body"
                        activeEpoch={delegationData?.stakeRequestEpoch}
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
