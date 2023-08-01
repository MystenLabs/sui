// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin } from '@mysten/core';
import { type CoinStruct } from '@mysten/sui.js/client';
import { Text } from '@mysten/ui';

import { ObjectLink } from '~/ui/InternalLink';

type CoinItemProps = {
	coin: CoinStruct;
};

export default function CoinItem({ coin }: CoinItemProps) {
	const [formattedBalance, symbol] = useFormatCoin(coin.balance, coin.coinType);
	return (
		<div className="flex items-center justify-between rounded-lg bg-white px-3 py-2 shadow-panel">
			<ObjectLink objectId={coin.coinObjectId} />
			<div className="col-span-3 inline-flex items-center gap-1">
				<Text color="steel-darker" variant="bodySmall/medium">
					{formattedBalance}
				</Text>
				<Text color="steel" variant="subtitleSmallExtra/normal">
					{symbol}
				</Text>
			</div>
		</div>
	);
}
