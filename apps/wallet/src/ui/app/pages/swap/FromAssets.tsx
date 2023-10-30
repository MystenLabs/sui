// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Loading from '_components/loading';
import Overlay from '_components/overlay';
import { filterAndSortTokenBalances } from '_helpers';
import { useActiveAddress, useCoinsReFetchingConfig, useSortedCoinsByCategories } from '_hooks';
import { TokenRow } from '_pages/home/tokens/TokensDetails';
import { useSuiClientQuery } from '@mysten/dapp-kit';
import { Fragment } from 'react';
import { useNavigate } from 'react-router-dom';

export function FromAssets() {
	const navigate = useNavigate();
	const selectedAddress = useActiveAddress();
	const { staleTime, refetchInterval } = useCoinsReFetchingConfig();

	const { data: coins, isPending } = useSuiClientQuery(
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
			<Loading loading={isPending}>
				<div className="flex flex-shrink-0 justify-start flex-col w-full">
					{recognized?.map((coinBalance, index) => {
						return (
							<Fragment key={coinBalance.coinType}>
								<TokenRow
									coinBalance={coinBalance}
									onClick={() => {
										navigate(
											`/swap?${new URLSearchParams({ type: coinBalance.coinType }).toString()}`,
										);
									}}
								/>

								{index !== recognized.length - 1 && <div className="bg-gray-45 h-px w-full" />}
							</Fragment>
						);
					})}
				</div>
			</Loading>
		</Overlay>
	);
}
