// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSortedCoinsByCategories } from '_app/hooks/useSortedCoinsByCategories';
import Loading from '_components/loading';
import Overlay from '_components/overlay';
import { filterAndSortTokenBalances } from '_helpers';
import { useActiveAddress, useCoinsReFetchingConfig } from '_hooks';
import { TokenRow } from '_pages/home/tokens/TokensDetails';
import { useSuiClientQuery } from '@mysten/dapp-kit';
import { useNavigate } from 'react-router-dom';

export function BaseAssets() {
	const navigate = useNavigate();
	const selectedAddress = useActiveAddress();
	const { staleTime, refetchInterval } = useCoinsReFetchingConfig();

	const { data: coins, isLoading } = useSuiClientQuery(
		'getAllBalances',
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
		<Overlay showModal title="Select a Coin" closeOverlay={() => navigate(-1)}>
			<Loading loading={isLoading}>
				<div className="flex flex-shrink-0 justify-start flex-col w-full">
					{recognized?.map((coinBalance, index) => {
						return (
							<TokenRow
								borderBottom={index !== recognized.length - 1}
								key={coinBalance.coinType}
								as="button"
								coinBalance={coinBalance}
								onClick={() => {
									navigate(
										`/swap?${new URLSearchParams({ type: coinBalance.coinType }).toString()}`,
									);
								}}
							/>
						);
					})}
				</div>
			</Loading>
		</Overlay>
	);
}
