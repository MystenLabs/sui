// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Route } from 'react-router-dom';

import StakeHome from './home';
import StakeNew from './stake';

export const routes = (
    <>
        <Route path="stake" element={<StakeHome />} />
        <Route path="stake/new" element={<StakeNew />} />
    </>
);
