// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useOnScreen, useGetCoins, useElementDimensions } from '@mysten/core';
import { LoadingIndicator } from '@mysten/ui';
import clsx from 'clsx';
import { useEffect, useRef } from 'react';

import CoinItem from './CoinItem';

const MIN_CONTAINER_WIDTH_SIZE = 500;

type CoinsPanelProps = {
	coinType: string;
	id: string;
};

export default function CoinsPanel({ coinType, id }: CoinsPanelProps) {
	const containerRef = useRef(null);
	const coinsSectionRef = useRef(null);
	const { isIntersecting } = useOnScreen(containerRef);
	const { data, isLoading, isFetching, fetchNextPage, hasNextPage } = useGetCoins(coinType, id);
	const [_, containerWidth] = useElementDimensions(coinsSectionRef);

	const isSpinnerVisible = hasNextPage || isLoading || isFetching;

	useEffect(() => {
		if (isIntersecting && hasNextPage && !isFetching) {
			fetchNextPage();
		}
	}, [isIntersecting, hasNextPage, isFetching, fetchNextPage]);

	return (
		<div className="pb-3">
			<div className="flex flex-wrap" ref={coinsSectionRef}>
				{data &&
					data.pages.map((page) =>
						page.data.map((coin) => (
							<div
								key={coin.coinObjectId}
								className={clsx(
									'w-full min-w-coinItemContainer pb-3 md:pr-3',
									containerWidth > MIN_CONTAINER_WIDTH_SIZE && 'basis-1/3',
								)}
							>
								<CoinItem coin={coin} />
							</div>
						)),
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
