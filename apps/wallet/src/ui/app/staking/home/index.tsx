// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Route, Routes } from 'react-router-dom';

import { useUnlockedGuard } from '../../hooks/useUnlockedGuard';
import { DelegationDetail } from '../delegation-detail';
import StakePage from '../stake';
import { Validators } from '../validators';

export function Staking() {
	if (useUnlockedGuard()) {
		return null;
	}
	return (
		<Routes>
			<Route path="/*" element={<Validators />} />
			<Route path="/delegation-detail" element={<DelegationDetail />} />
			<Route path="/new" element={<StakePage />} />
		</Routes>
	);
}
