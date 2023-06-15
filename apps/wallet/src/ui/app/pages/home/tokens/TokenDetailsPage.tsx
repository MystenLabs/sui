// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Navigate, useSearchParams } from 'react-router-dom';

import TokenDetails from './TokensDetails';

function TokenDetailsPage() {
	const [searchParams] = useSearchParams();
	const coinType = searchParams.get('type');

	if (!coinType) {
		return <Navigate to="/tokens" replace={true} />;
	}
	return <TokenDetails coinType={coinType} />;
}

export default TokenDetailsPage;
