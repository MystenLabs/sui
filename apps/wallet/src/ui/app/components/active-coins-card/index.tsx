// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetAllBalances } from '@mysten/core';
import { SUI_TYPE_ARG } from '@mysten/sui.js';
import { Link } from 'react-router-dom';

import { CoinItem } from './CoinItem';
import { useActiveAddress } from '_app/hooks/useActiveAddress';
import Loading from '_components/loading';
import { filterAndSortTokenBalances } from '_helpers';
import { useCoinsReFetchingConfig } from '_hooks';

export function ActiveCoinsCard({
	activeCoinType = SUI_TYPE_ARG,
	showActiveCoin = true,
}: {
	activeCoinType: string;
	showActiveCoin?: boolean;
}) {
	const selectedAddress = useActiveAddress();

	const { staleTime, refetchInterval } = useCoinsReFetchingConfig();
	const { data: coins, isLoading } = useGetAllBalances(
		selectedAddress!,
		refetchInterval,
		staleTime,
		filterAndSortTokenBalances,
	);

	const activeCoin = coins?.find(({ coinType }) => coinType === activeCoinType);

	return (
		<Loading loading={isLoading}>
			<div className="flex w-full">
				{showActiveCoin ? (
					activeCoin && (
						<Link
							to={`/send/select?${new URLSearchParams({
								type: activeCoin.coinType,
							}).toString()}`}
							className="border-solid border border-gray-45 rounded-2lg no-underline flex gap-2 items-center w-full overflow-hidden"
						>
							<CoinItem
								coinType={activeCoin.coinType}
								balance={BigInt(activeCoin.totalBalance)}
								isActive
							/>
						</Link>
					)
				) : (
					<div className="flex flex-col w-full">
						<div className="flex flex-col justify-between items-center mt-2 divide-y divide-solid divide-gray-45 divide-x-0">
							{coins?.map(({ coinType, totalBalance }) => (
								<Link
									to={`/send?${new URLSearchParams({
										type: coinType,
									}).toString()}`}
									key={coinType}
									className="no-underline w-full"
								>
									<CoinItem coinType={coinType} balance={BigInt(totalBalance)} />
								</Link>
							))}
						</div>
					</div>
				)}
			</div>
		</Loading>
	);
}
