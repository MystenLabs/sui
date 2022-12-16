// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useState } from 'react';
import { useSearchParams, useNavigate } from 'react-router-dom';

import { SelectValidatorCard } from './SelectValidatorCard';
import StakingCard from './StakingCard';
import { SuiIcons } from '_components/icon';
import Overlay from '_components/overlay';

function StakePage() {
    const [searchParams] = useSearchParams();
    const validatorAddress = searchParams.get('address');
    const [showModal, setShowModal] = useState(true);

    const navigate = useNavigate();
    const close = useCallback(() => {
        navigate('/');
    }, [navigate]);

    return (
        <Overlay
            showModal={showModal}
            setShowModal={setShowModal}
            title={validatorAddress ? 'Stake & Earn SUI' : 'Select a Validator'}
            closeIcon={SuiIcons.Close}
            closeOverlay={close}
        >
            {validatorAddress ? <StakingCard /> : <SelectValidatorCard />}
        </Overlay>
    );
}

export default StakePage;
