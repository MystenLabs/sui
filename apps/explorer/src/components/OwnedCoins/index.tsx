// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetAllBalances } from '@mysten/core';
import { Heading, Text, LoadingIndicator } from '@mysten/ui';
import { useState } from 'react';

import OwnedCoinView from './OwnedCoinView';
import { Pagination } from '~/ui/Pagination';

export const COINS_PER_PAGE: number = 6;

export function OwnedCoins({ id }: { id: string }) {
	const [currentSlice, setCurrentSlice] = useState(1);
	const { isLoading, data, isError } = useGetAllBalances(id);

	if (isError) {
		return <div className="pt-2 font-sans font-semibold text-issue-dark">Failed to load Coins</div>;
	}

	return (
		<div>
			{isLoading ? (
				<LoadingIndicator />
			) : (
				<div className="flex flex-col gap-4 pt-5 text-left">
					<Heading color="steel-darker" variant="heading4/semibold">
						Coins
					</Heading>
					<div className="flex max-h-80 flex-col overflow-auto">
						<div className="grid grid-cols-3 py-2 uppercase tracking-wider text-gray-80">
							<Text variant="caption/medium">Type</Text>
							<Text variant="caption/medium">Objects</Text>
							<Text variant="caption/medium">Balance</Text>
						</div>
						<div>
							{data
								.slice((currentSlice - 1) * COINS_PER_PAGE, currentSlice * COINS_PER_PAGE)
								.map((coin) => (
									<OwnedCoinView id={id} key={coin.coinType} coin={coin} />
								))}
						</div>
					</div>
					{data.length > COINS_PER_PAGE && (
						<Pagination
							onNext={() => setCurrentSlice(currentSlice + 1)}
							hasNext={currentSlice !== Math.ceil(data.length / COINS_PER_PAGE)}
							hasPrev={currentSlice !== 1}
							onPrev={() => setCurrentSlice(currentSlice - 1)}
							onFirst={() => setCurrentSlice(1)}
						/>
					)}
				</div>
			)}
		</div>
	);
}
