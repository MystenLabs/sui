// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useUnlockedGuard } from '_src/ui/app/hooks/useUnlockedGuard';
import { Route, Routes } from 'react-router-dom';

import { HiddenAssetsPage, NftsPage } from '..';
import { HiddenAssetsProvider } from '../hidden-assets/HiddenAssetsProvider';

function AssetsPage() {
	if (useUnlockedGuard()) {
		return null;
	}
	return (
		<HiddenAssetsProvider>
			<Routes>
				<Route path="/hidden-assets" element={<HiddenAssetsPage />} />
				<Route path="/:filterType?/*" element={<NftsPage />} />
			</Routes>
		</HiddenAssetsProvider>
	);
}

export default AssetsPage;
