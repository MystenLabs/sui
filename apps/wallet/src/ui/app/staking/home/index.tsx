// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { useEffect } from 'react';
import { Navigate, Route, Routes, useNavigate } from 'react-router-dom';

import { DelegationDetail } from '../delegation-detail';
import StakePage from '../stake';
import { Validators } from '../validators';
import { FEATURES } from '_src/shared/experimentation/features';

export function Staking() {
    const navigate = useNavigate();
    const { source, on } = useFeature(FEATURES.STAKING_ENABLED);

    // Handle the case where features take too long to load, and we'll just navigate home:
    useEffect(() => {
        if (source !== 'defaultValue') return;

        const timeout = setTimeout(() => {
            navigate('/', { replace: true });
        }, 5000);

        return () => {
            clearTimeout(timeout);
        };
    }, [source, navigate]);

    // Wait for features to load
    if (source === 'defaultValue') {
        return null;
    }

    if (!on) return <Navigate to="/" replace />;

    return (
        <Routes>
            <Route path="/*" element={<Validators />} />
            <Route path="/delegation-detail" element={<DelegationDetail />} />
            <Route path="/new" element={<StakePage />} />
        </Routes>
    );
}
