// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { Route, Routes } from 'react-router-dom';

import { FEATURES } from '../../experimentation/features';
import StakeNew from '../stake';
import { ValidatorDetail } from '../validator-detail';
import StakeHome from './Stake';

export function Staking() {
    const stakingEnabled = useFeature(FEATURES.STAKING_ENABLED).on;

    return (
        <Routes>
            <Route path="/*" element={<StakeHome />} />
            <Route path="/validator-details" element={<ValidatorDetail />} />
            {stakingEnabled ? (
                <Route path="/new" element={<StakeNew />} />
            ) : null}
        </Routes>
    );
}
