// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Routes, Route } from 'react-router-dom';
import { HiddenAssetsPage, NftsPage } from '..';
import { HiddenAssetsProvider } from '../hidden-assets/HiddenAssetsProvider';

function AssetsPage() {
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
