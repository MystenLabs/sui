// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Sui } from '@mysten/icons';
import { Text } from '@mysten/ui';

import { useSuiCoinData } from '~/hooks/useSuiCoinData';
import { Card } from '~/ui/Card';
import { ButtonOrLink } from '~/ui/utils/ButtonOrLink';

const COIN_GECKO_SUI_URL = 'https://www.coingecko.com/en/coins/sui';

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
		<ButtonOrLink href={COIN_GECKO_SUI_URL}>
			<Card growOnHover bg="white/80" spacing="lg" height="full">
				<div className="flex items-center gap-2">
					<div className="h-5 w-5 flex-shrink-0 rounded-full bg-sui p-1">
						<Sui className="h-full w-full text-white" />
					</div>
					<div className="flex w-full flex-col gap-0.5">
						<Text variant="body/semibold" color="steel-darker">
							1 SUI = {formattedPrice}
						</Text>
						<Text variant="subtitleSmallExtra/medium" color="steel">
							via CoinGecko
						</Text>
					</div>
				</div>
			</Card>
		</ButtonOrLink>
	);
}
