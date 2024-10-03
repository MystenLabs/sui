// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetAllBalances } from '_app/hooks/useGetAllBalances';
import Loading from '_components/loading';
import Overlay from '_components/overlay';
import { useActiveAddress, useSortedCoinsByCategories } from '_hooks';
import { TokenRow } from '_pages/home/tokens/TokensDetails';
import { normalizeStructTag } from '@mysten/sui/utils';
import { Fragment } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';

export function CoinsSelectionPage() {
	const navigate = useNavigate();
	const selectedAddress = useActiveAddress();
	const [searchParams] = useSearchParams();
	const allowedCoinTypes = (searchParams.get('allowedCoinTypes') || '').split(',');
	const fromCoinType = searchParams.get('fromCoinType');
	const toCoinType = searchParams.get('toCoinType');

	const { data: coinBalances, isPending } = useGetAllBalances(selectedAddress || '');

	const { recognized } = useSortedCoinsByCategories(coinBalances ?? []);

	return (
		<Overlay showModal title="Select a Coin" closeOverlay={() => navigate(-1)}>
			<Loading loading={isPending}>
				<div className="flex flex-shrink-0 justify-start flex-col w-full">
					{allowedCoinTypes.map((coinType, index) => {
						const coinBalance = recognized?.find((coin) => coin.coinType === coinType) || {};
						const totalBalance =
							coinBalances?.find(
								(balance) => normalizeStructTag(balance.coinType) === normalizeStructTag(coinType),
							)?.totalBalance ?? '0';

						return (
							<Fragment key={coinType}>
								<TokenRow
									coinBalance={{
										coinType,
										coinObjectCount: 0,
										lockedBalance: {},
										totalBalance,
										...coinBalance,
									}}
									onClick={() => {
										const params = fromCoinType
											? { type: fromCoinType, toType: coinType }
											: { type: coinType, toType: toCoinType || '' };
										navigate(`/swap?${new URLSearchParams(params)}`);
									}}
								/>

								<div className="bg-gray-45 h-px w-full" />
							</Fragment>
						);
					})}
				</div>
			</Loading>
		</Overlay>
	);
}
