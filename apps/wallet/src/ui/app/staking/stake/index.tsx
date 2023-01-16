// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import { useSearchParams, useNavigate } from 'react-router-dom';

import { SelectValidatorCard } from '../validators/SelectValidatorCard';
import StakingCard from './StakingCard';
import { SuiIcons } from '_components/icon';
import Overlay from '_components/overlay';

function StakePage() {
    const [searchParams] = useSearchParams();
    const validatorAddress = searchParams.get('address');
    const [showModal, setShowModal] = useState(true);
    const unstake = searchParams.get('unstake') === 'true';

    const navigate = useNavigate();
    const close = () => {
        navigate('/');
    };

    const stakingTitle = unstake ? 'Unstake SUI' : 'Stake SUI';

    return (
        <Overlay
            showModal={showModal}
            setShowModal={setShowModal}
            title={validatorAddress ? stakingTitle : 'Select a Validator'}
            closeIcon={SuiIcons.Close}
            closeOverlay={close}
        >
            {validatorAddress ? <StakingCard /> : <SelectValidatorCard />}
        </Overlay>
    );
}

export default StakePage;
