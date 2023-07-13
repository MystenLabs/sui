// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Sui } from '@mysten/icons';

import { useSuiCoinData } from '~/hooks/useSuiCoinData';
import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';
import { Text } from '~/ui/Text';

export function SuiTokenCard() {
	const { data } = useSuiCoinData();
	const { currentPrice } = data || {};

	const formattedPrice = currentPrice
		? currentPrice.toLocaleString('en', {
				style: 'currency',
				currency: 'USD',
		  })
		: '--';

	return (
		<Card bg="white" spacing="lg" height="full">
			<div className="flex items-center gap-2">
				<div className="h-5 w-5 flex-shrink-0 rounded-full bg-sui p-1">
					<Sui className="h-full w-full text-white" />
				</div>
				<div className="flex w-full flex-col gap-0.5">
					<Heading variant="heading4/semibold" color="steel-darker">
						1 SUI = {formattedPrice}
					</Heading>
					<Text variant="subtitleSmallExtra/medium" color="steel">
						via CoinGecko
					</Text>
				</div>
			</div>
		</Card>
	);
}
