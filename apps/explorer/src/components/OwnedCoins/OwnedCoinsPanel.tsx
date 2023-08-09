// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useOnScreen, useGetCoins } from '@mysten/core';
import { LoadingIndicator } from '@mysten/ui';
import { useEffect, useRef } from 'react';

import CoinItem from './CoinItem';

type CoinsPanelProps = {
	coinType: string;
	id: string;
};

export default function CoinsPanel({ coinType, id }: CoinsPanelProps) {
	const containerRef = useRef(null);
	const { isIntersecting } = useOnScreen(containerRef);
	const { data, isLoading, isFetching, fetchNextPage, hasNextPage } = useGetCoins(coinType, id);

	const isSpinnerVisible = hasNextPage || isLoading || isFetching;

	useEffect(() => {
		if (isIntersecting && hasNextPage && !isFetching) {
			fetchNextPage();
		}
	}, [isIntersecting, hasNextPage, isFetching, fetchNextPage]);

	return (
		<div className="pb-3">
			<div className="grid grid-cols-1 gap-3 px-6 md:grid-cols-2">
				{data &&
					data.pages.map((page) =>
						page.data.map((coin) => <CoinItem key={coin.coinObjectId} coin={coin} />),
					)}
			</div>
			{isSpinnerVisible && (
				<div className="flex justify-center" ref={containerRef}>
					<div className="mt-5 flex">
						<LoadingIndicator />
					</div>
				</div>
			)}
		</div>
	);
}
