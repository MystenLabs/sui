// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getSendOrSwapUrl } from '_app/helpers/getSendOrSwapUrl';
import { useSortedCoinsByCategories } from '_app/hooks/useSortedCoinsByCategories';
import Overlay from '_components/overlay';
import { filterAndSortTokenBalances } from '_helpers';
import { useActiveAddress, useCoinsReFetchingConfig } from '_hooks';
import { TokenRow } from '_pages/home/tokens/TokensDetails';
import { useAllBalances } from '@mysten/dapp-kit';
import { useNavigate } from 'react-router-dom';

export function Assets() {
	const navigate = useNavigate();
	const selectedAddress = useActiveAddress();
	const { staleTime, refetchInterval } = useCoinsReFetchingConfig();

	const { data: coins } = useAllBalances(
		{ owner: selectedAddress! },
		{
			enabled: !!selectedAddress,
			refetchInterval,
			staleTime,
			select: filterAndSortTokenBalances,
		},
	);

	const { recognized } = useSortedCoinsByCategories(coins ?? []);

	return (
		<Overlay showModal title="Swap" closeOverlay={() => navigate(-1)}>
			<div className="flex flex-shrink-0 justify-start flex-col w-full">
				{recognized?.map((coinBalance) => (
					<TokenRow
						key={coinBalance.coinType}
						as="button"
						coinBalance={coinBalance}
						onClick={() => {
							navigate(getSendOrSwapUrl('swap', coinBalance.coinType));
						}}
					/>
				))}
			</div>
		</Overlay>
	);
}
