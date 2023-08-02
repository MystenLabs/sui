// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetAllBalances } from '@mysten/core';
import { Info16 } from '@mysten/icons';
import { type CoinBalance } from '@mysten/sui.js';
import { Coin } from '@mysten/sui.js';
import { Heading, Text, LoadingIndicator, RadioGroup, RadioGroupItem } from '@mysten/ui';
import { useMemo, useState } from 'react';

import OwnedCoinView from './OwnedCoinView';
import { useRecognizedPackages } from '~/hooks/useRecognizedPackages';
import { Pagination } from '~/ui/Pagination';

export type CoinBalanceVerified = CoinBalance & {
	isRecognized?: boolean;
};

enum COIN_FILTERS {
	ALL = 'allBalances',
	RECOGNIZED = 'recognizedBalances',
	UNRECOGNIZED = 'unrecognizedBalances',
}

export function OwnedCoins({ id }: { id: string }) {
	const [currentSlice, setCurrentSlice] = useState(1);
	const [limit, setLimit] = useState(20);
	const [filterValue, setFilterValue] = useState(COIN_FILTERS.RECOGNIZED);
	const { isLoading, data, isError } = useGetAllBalances(id);
	const recognizedPackages = useRecognizedPackages();

	const balances: Record<COIN_FILTERS, CoinBalanceVerified[]> = useMemo(() => {
		const balanceData = data?.reduce(
			(acc, coinBalance) => {
				if (recognizedPackages.includes(coinBalance.coinType.split('::')[0])) {
					acc.recognizedBalances.push({
						...coinBalance,
						isRecognized: true,
					});
				} else {
					acc.unrecognizedBalances.push({ ...coinBalance, isRecognized: false });
				}
				return acc;
			},
			{
				recognizedBalances: [] as CoinBalanceVerified[],
				unrecognizedBalances: [] as CoinBalanceVerified[],
			},
		) ?? { recognizedBalances: [], unrecognizedBalances: [] };

		const recognizedBalances = balanceData.recognizedBalances.sort((a, b) => {
			// Make sure SUI always comes first
			if (Coin.getCoinSymbol(a.coinType) === 'SUI') {
				return -1;
			} else if (Coin.getCoinSymbol(b.coinType) === 'SUI') {
				return 1;
			} else {
				return Coin.getCoinSymbol(a.coinType).localeCompare(
					Coin.getCoinSymbol(b.coinType),
					undefined,
					{ sensitivity: 'base' },
				);
			}
		});

		return {
			recognizedBalances,
			unrecognizedBalances: balanceData.unrecognizedBalances.sort((a, b) =>
				Coin.getCoinSymbol(a.coinType).localeCompare(Coin.getCoinSymbol(b.coinType), undefined, {
					sensitivity: 'base',
				}),
			),
			allBalances: balanceData.recognizedBalances.concat(balanceData.unrecognizedBalances),
		};
	}, [data, recognizedPackages]);

	const filterOptions = useMemo(
		() => [
			{ label: 'ALL', value: COIN_FILTERS.ALL },
			{ label: `${balances.recognizedBalances.length} RECOGNIZED`, value: COIN_FILTERS.RECOGNIZED },
			{
				label: `${balances.unrecognizedBalances.length} UNRECOGNIZED`,
				value: COIN_FILTERS.UNRECOGNIZED,
			},
		],
		[balances],
	);

	const displayedBalances = useMemo(() => balances[filterValue], [balances, filterValue]);

	if (isError) {
		return <div className="pt-2 font-sans font-semibold text-issue-dark">Failed to load Coins</div>;
	}

	return (
		<div className="h-full w-full">
			{isLoading ? (
				<div className="m-auto flex h-full w-full justify-center text-white">
					<LoadingIndicator />
				</div>
			) : (
				<div className="flex flex-col gap-4 pt-5 text-left">
					<div className='md:mt-12" flex w-full justify-between border-b border-gray-45 pb-3'>
						<Heading color="steel-darker" variant="heading4/semibold">
							{balances.allBalances.length} Coins
						</Heading>
						<div>
							<RadioGroup
								aria-label="transaction filter"
								value={filterValue}
								onValueChange={(value) => setFilterValue(value as COIN_FILTERS)}
							>
								{filterOptions.map((filter) => (
									<RadioGroupItem
										key={filter.value}
										value={filter.value}
										label={filter.label}
										disabled={!balances[filter.value].length}
									/>
								))}
							</RadioGroup>
						</div>
					</div>
					{filterValue === COIN_FILTERS.UNRECOGNIZED && (
						<div className="flex items-center gap-2 rounded-full border border-gray-45 p-2 text-steel-darker">
							<div>
								<Info16 width="16px" />
							</div>
							<Text color="steel-darker" variant="body/medium">
								These coins have not been recognized by Sui Foundation.
							</Text>
						</div>
					)}

					<div className="flex max-h-80 flex-col overflow-auto">
						<div className="mb-2.5 flex uppercase tracking-wider text-gray-80">
							<div className="w-[45%] pl-3">
								<Text variant="caption/medium" color="steel-dark">
									Type
								</Text>
							</div>
							<div className="w-[25%] px-2">
								<Text variant="caption/medium" color="steel-dark">
									Objects
								</Text>
							</div>
							<div className="w-[30%]">
								<Text variant="caption/medium" color="steel-dark">
									Balance
								</Text>
							</div>
						</div>
						<div>
							{displayedBalances
								.slice((currentSlice - 1) * limit, currentSlice * limit)
								.map((coin) => (
									<OwnedCoinView id={id} key={coin.coinType} coin={coin} />
								))}
						</div>
					</div>
					{displayedBalances.length > limit && (
						<div className="flex flex-col justify-between gap-2 md:flex-row">
							<Pagination
								onNext={() => setCurrentSlice(currentSlice + 1)}
								hasNext={currentSlice !== Math.ceil(displayedBalances.length / limit)}
								hasPrev={currentSlice !== 1}
								onPrev={() => setCurrentSlice(currentSlice - 1)}
								onFirst={() => setCurrentSlice(1)}
							/>
							<div className="flex items-center gap-3">
								<Text variant="body/medium" color="steel-dark">
									{`Showing `}
									{(currentSlice - 1) * limit + 1}-
									{currentSlice * limit > displayedBalances.length
										? displayedBalances.length
										: currentSlice * limit}
								</Text>
								<select
									className="form-select flex rounded-md border border-gray-45 px-3 py-2 pr-8 text-bodySmall font-medium leading-[1.2] text-steel-dark shadow-button"
									value={limit}
									onChange={(e) => {
										setLimit(Number(e.target.value));
										setCurrentSlice(1);
									}}
								>
									<option value={20}>20 Per Page</option>
									<option value={40}>40 Per Page</option>
									<option value={60}>60 Per Page</option>
								</select>
							</div>
						</div>
					)}
				</div>
			)}
		</div>
	);
}
