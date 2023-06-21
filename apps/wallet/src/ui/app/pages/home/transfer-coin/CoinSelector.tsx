// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_TYPE_ARG } from '@mysten/sui.js';
import { useSearchParams, useNavigate } from 'react-router-dom';

import { ActiveCoinsCard } from '_components/active-coins-card';
import Overlay from '_components/overlay';

function CoinsSelectorPage() {
	const [searchParams] = useSearchParams();
	const coinType = searchParams.get('type') || SUI_TYPE_ARG;
	const navigate = useNavigate();

	return (
		<Overlay
			showModal={true}
			title="Select Coin"
			closeOverlay={() =>
				navigate(
					`/send?${new URLSearchParams({
						type: coinType,
					}).toString()}`,
				)
			}
		>
			<ActiveCoinsCard activeCoinType={coinType} showActiveCoin={false} />
		</Overlay>
	);
}

export default CoinsSelectorPage;
