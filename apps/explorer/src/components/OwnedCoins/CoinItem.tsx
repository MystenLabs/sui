// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin } from '@mysten/core';
import { type CoinStruct } from '@mysten/sui.js';
import { Text } from '@mysten/ui';

import { ObjectLink } from '~/ui/InternalLink';

type CoinItemProps = {
	coin: CoinStruct;
};

export default function CoinItem({ coin }: CoinItemProps) {
	const [formattedBalance, symbol] = useFormatCoin(coin.balance, coin.coinType);
	return (
		<div className="bg-grey-40 grid grid-flow-row auto-rows-fr grid-cols-4 items-center">
			<Text color="steel-darker" variant="bodySmall/medium">
				Object ID
			</Text>
			<div className="col-span-3">
				<ObjectLink objectId={coin.coinObjectId} noTruncate />
			</div>

			<Text color="steel-darker" variant="bodySmall/medium">
				Balance
			</Text>

			<div className="col-span-3 inline-flex items-end gap-1">
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
