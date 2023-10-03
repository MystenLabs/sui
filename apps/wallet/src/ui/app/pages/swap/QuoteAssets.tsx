// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { coinsMap } from '_app/hooks/useDeepBook';
import Overlay from '_components/overlay';
import { useActiveAddress, useCoinsReFetchingConfig } from '_hooks';
import { TokenRow } from '_pages/home/tokens/TokensDetails';
import { useSuiClientQuery } from '@mysten/dapp-kit';
import { Fragment } from 'react';
import { useSearchParams } from 'react-router-dom';

const recognizedCoins = Object.values(coinsMap);

function QuoteAsset({
	coinType,
	onClick,
}: {
	coinType: string;
	onClick: (coinType: string) => void;
}) {
	const accountAddress = useActiveAddress();
	const [searchParams] = useSearchParams();
	const activeCoinType = searchParams.get('type');

	const { staleTime, refetchInterval } = useCoinsReFetchingConfig();

	const { data: coinBalance } = useSuiClientQuery(
		'getBalance',
		{ coinType: coinType, owner: accountAddress! },
		{ enabled: !!accountAddress, refetchInterval, staleTime },
	);

	if (!coinBalance || coinBalance.coinType === activeCoinType) {
		return null;
	}

	return (
		<TokenRow
			coinBalance={coinBalance}
			onClick={() => {
				onClick(coinType);
			}}
		/>
	);
}

export function QuoteAssets({
	setOpen,
	isOpen,
	onRowClick,
}: {
	setOpen: (isOpen: boolean) => void;
	isOpen: boolean;
	onRowClick: (coinType: string) => void;
}) {
	return (
		<Overlay showModal={isOpen} title="Select a Coin" closeOverlay={() => setOpen(false)}>
			<div className="flex flex-shrink-0 justify-start flex-col w-full">
				{recognizedCoins.map((coinType, index) => (
					<Fragment key={coinType}>
						<QuoteAsset coinType={coinType} onClick={onRowClick} />
						{index !== recognizedCoins.length - 1 && <div className="bg-gray-45 h-px w-full" />}
					</Fragment>
				))}
			</div>
		</Overlay>
	);
}
