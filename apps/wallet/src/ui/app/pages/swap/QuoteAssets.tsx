// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getSendOrSwapUrl } from '_app/helpers/getSendOrSwapUrl';
import { coinsMap } from '_app/hooks/useDeepbook';
import Overlay from '_components/overlay';
import { useActiveAddress, useCoinsReFetchingConfig } from '_hooks';
import { TokenRow } from '_pages/home/tokens/TokensDetails';
import { useBalance } from '@mysten/dapp-kit';
import { useNavigate, useSearchParams } from 'react-router-dom';

const recognizedCoins = Object.values(coinsMap);

function QuoteAsset({ coinType, borderBottom }: { coinType: string; borderBottom?: boolean }) {
	const navigate = useNavigate();
	const accountAddress = useActiveAddress();
	const [searchParams] = useSearchParams();
	const activeCoinType = searchParams.get('type');

	const { staleTime, refetchInterval } = useCoinsReFetchingConfig();

	const { data: coinBalance } = useBalance(
		{ coinType: coinType, owner: accountAddress! },
		{ enabled: !!accountAddress, refetchInterval, staleTime },
	);

	if (!coinBalance || coinBalance.coinType === activeCoinType) {
		return null;
	}

	return (
		<TokenRow
			as="button"
			borderBottom={borderBottom}
			coinBalance={coinBalance}
			onClick={() => {
				navigate(getSendOrSwapUrl('swap', activeCoinType || '', coinType));
			}}
		/>
	);
}

export function QuoteAssets() {
	const navigate = useNavigate();

	return (
		<Overlay showModal title="Select a Coin" closeOverlay={() => navigate(-1)}>
			<div className="flex flex-shrink-0 justify-start flex-col w-full">
				{recognizedCoins.map((coinType, index) => (
					<QuoteAsset
						key={coinType}
						borderBottom={index !== recognizedCoins.length - 1}
						coinType={coinType}
					/>
				))}
			</div>
		</Overlay>
	);
}
