// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { coinsMap } from '_app/hooks/useDeepBook';
import Overlay from '_components/overlay';
import { useActiveAddress, useCoinsReFetchingConfig } from '_hooks';
import { TokenRow } from '_pages/home/tokens/TokensDetails';
import { useSuiClientQuery } from '@mysten/dapp-kit';
import { useSearchParams } from 'react-router-dom';

const recognizedCoins = Object.values(coinsMap);

function QuoteAsset({
	coinType,
	borderBottom,
	onClick,
}: {
	coinType: string;
	onClick: (coinType: string) => void;
	borderBottom?: boolean;
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
			as="button"
			borderBottom={borderBottom}
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
					<QuoteAsset
						key={coinType}
						borderBottom={index !== recognizedCoins.length - 1}
						coinType={coinType}
						onClick={onRowClick}
					/>
				))}
			</div>
		</Overlay>
	);
}
