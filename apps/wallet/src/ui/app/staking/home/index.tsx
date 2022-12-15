// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { Route, Routes } from 'react-router-dom';

import { usePendingDelegation } from '../usePendingDelegation';
import { ActiveDelegation } from './ActiveDelegation';
import { DelegationCard, DelegationState } from './DelegationCard';
import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import CoinBalance from '_app/shared/coin-balance';
import PageTitle from '_app/shared/page-title';
import StatsCard, { StatsRow, StatsItem } from '_app/shared/stats-card';
import {
    activeDelegationIDsSelector,
    totalActiveStakedSelector,
} from '_app/staking/selectors';
import Alert from '_components/alert';
import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import { useAppSelector, useObjectsState } from '_hooks';
import { GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';
import { FEATURES } from '_src/shared/experimentation/features';

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
