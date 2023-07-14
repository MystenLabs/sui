// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Routes, Route } from 'react-router-dom';
import { HiddenAssetsPage, NftsPage } from '..';

function AssetsPage() {
	return (
		<Routes>
			<Route path="/hidden-assets" element={<HiddenAssetsPage />} />
			<Route path="/*" element={<NftsPage />} />
		</Routes>
	);
}

export default AssetsPage;
