// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Route, Routes } from 'react-router-dom';

import TokenDetailsPage from './TokenDetailsPage';
import TokenDetails from './TokensDetails';

function TokensPage() {
	return (
		<Routes>
			<Route path="/" element={<TokenDetails />} />
			<Route path="/details" element={<TokenDetailsPage />} />
		</Routes>
	);
}

export default TokensPage;
