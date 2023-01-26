// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { Navigate, Route, Routes } from 'react-router-dom';

import { DelegationDetail } from '../delegation-detail';
import StakePage from '../stake';
import { Validators } from '../validators';
import { FEATURES } from '_src/shared/experimentation/features';

export function Staking() {
    const stakingEnabled = useFeature(FEATURES.STAKING_ENABLED).on;

    if (!stakingEnabled) return <Navigate to="/" replace />;

    return (
        <Routes>
            <Route path="/*" element={<Validators />} />
            <Route path="/delegation-detail" element={<DelegationDetail />} />
            <Route path="/new" element={<StakePage />} />
        </Routes>
    );
}
