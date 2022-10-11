// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Route } from 'react-router-dom';

import StakeHome from './home';
import StakeNew from './stake';

export const STAKING_ENABLED = process.env.STAKING_ENABLED === 'true';

export const routes = (
    <>
        <Route path="stake" element={<StakeHome />} />
        {STAKING_ENABLED ? (
            <Route path="stake/new" element={<StakeNew />} />
        ) : null}
    </>
);
