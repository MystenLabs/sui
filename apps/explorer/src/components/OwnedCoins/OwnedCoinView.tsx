// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin } from '@mysten/core';
import { ArrowShowAndHideRight12 } from '@mysten/icons';
import { type CoinBalance } from '@mysten/sui.js';
import { Text } from '@mysten/ui';
import * as Collapsible from '@radix-ui/react-collapsible';
import clsx from 'clsx';
import { useState } from 'react';

import CoinsPanel from './OwnedCoinsPanel';

type OwnedCoinViewProps = {
	coin: CoinBalance;
	id: string;
};

export default function OwnedCoinView({ coin, id }: OwnedCoinViewProps) {
	const [open, setOpen] = useState(false);
	const [formattedTotalBalance, symbol] = useFormatCoin(coin.totalBalance, coin.coinType);

	return (
		<Collapsible.Root open={open} onOpenChange={setOpen}>
			<Collapsible.Trigger
				data-testid="ownedcoinlabel"
				className="grid w-full grid-cols-3 items-center justify-between rounded-none py-2 text-left hover:bg-sui-light"
			>
				<div className="flex">
					<ArrowShowAndHideRight12
						className={clsx('mr-1.5 text-gray-60', open && 'rotate-90 transform')}
					/>
					<Text color="steel-darker" variant="body/medium">
						{symbol}
					</Text>
				</div>

				<Text color="steel-darker" variant="body/medium">
					{coin.coinObjectCount}
				</Text>

				<div className="flex items-center gap-1">
					<Text color="steel-darker" variant="bodySmall/medium">
						{formattedTotalBalance}
					</Text>
					<Text color="steel" variant="subtitleSmallExtra/normal">
						{symbol}
					</Text>
				</div>
			</Collapsible.Trigger>

			<Collapsible.Content>
				<div className="flex flex-col gap-1 bg-gray-40 p-3">
					<CoinsPanel id={id} coinType={coin.coinType} />
				</div>
			</Collapsible.Content>
		</Collapsible.Root>
	);
}
