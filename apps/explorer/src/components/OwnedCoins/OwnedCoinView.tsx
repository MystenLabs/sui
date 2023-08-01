// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin } from '@mysten/core';
import { ArrowShowAndHideRight12 } from '@mysten/icons';
import { type CoinBalance } from '@mysten/sui.js/client';
import { Text } from '@mysten/ui';
import * as Collapsible from '@radix-ui/react-collapsible';
import clsx from 'clsx';
import { useState } from 'react';

import { CoinIcon } from './CoinIcon';
import CoinsPanel from './OwnedCoinsPanel';
import { Banner } from '~/ui/Banner';

type OwnedCoinViewProps = {
	coin: CoinBalance;
	id: string;
	isRecognized?: boolean;
};

export default function OwnedCoinView({ coin, id, isRecognized }: OwnedCoinViewProps) {
	const [open, setOpen] = useState(false);
	const [formattedTotalBalance, symbol] = useFormatCoin(coin.totalBalance, coin.coinType);

	return (
		<Collapsible.Root open={open} onOpenChange={setOpen}>
			<Collapsible.Trigger
				data-testid="ownedcoinlabel"
				className={clsx(
					'flex w-full items-center rounded-lg bg-opacity-5 py-2 text-left hover:bg-hero-darkest hover:bg-opacity-5',
					open && 'bg-hero-darkest',
				)}
				style={{
					borderBottomLeftRadius: open ? '0' : '8px',
					borderBottomRightRadius: open ? '0' : '8px',
				}}
			>
				<div className="flex w-[45%] items-center gap-1">
					<ArrowShowAndHideRight12
						className={clsx('text-gray-60', open && 'rotate-90 transform')}
					/>
					<div className="flex items-center gap-3">
						<CoinIcon coinType={coin.coinType} size="sm" />
						<Text color="steel-darker" variant="body/medium">
							{symbol}
						</Text>
					</div>

					{!isRecognized && (
						<Banner variant="warning" icon={null} border spacing="sm">
							<div className="max-w-[70px] overflow-hidden truncate whitespace-nowrap text-captionSmallExtra font-medium uppercase leading-3 tracking-wider lg:max-w-full">
								Unrecognized
							</div>
						</Banner>
					)}
				</div>

				<div className="flex w-[25%]">
					<Text color="steel-darker" variant="body/medium">
						{coin.coinObjectCount}
					</Text>
				</div>

				<div className="flex w-[30%] items-center gap-1">
					<Text color="steel-darker" variant="bodySmall/medium">
						{formattedTotalBalance}
					</Text>
					<Text color="steel" variant="subtitleSmallExtra/normal">
						{symbol}
					</Text>
				</div>
			</Collapsible.Trigger>

			<Collapsible.Content>
				<div
					className="flex flex-col gap-1 bg-gray-40 p-3"
					style={{
						borderBottomLeftRadius: '8px',
						borderBottomRightRadius: '8px',
					}}
				>
					<CoinsPanel id={id} coinType={coin.coinType} />
				</div>
			</Collapsible.Content>
		</Collapsible.Root>
	);
}
