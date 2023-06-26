// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '../../text';
import { CoinIcon } from '_src/ui/app/components/coin-icon';

export interface CoinsStackProps {
	coinTypes: string[];
}

const MAX_COINS_TO_DISPLAY = 4;

export function CoinsStack({ coinTypes }: CoinsStackProps) {
	return (
		<div className="flex">
			{coinTypes.length > MAX_COINS_TO_DISPLAY && (
				<Text variant="bodySmall" weight="medium" color="steel-dark">
					+{coinTypes.length - MAX_COINS_TO_DISPLAY}
				</Text>
			)}
			{coinTypes.slice(0, MAX_COINS_TO_DISPLAY).map((coinType, i) => (
				<div key={coinType} className={i === 0 ? '' : '-ml-1'}>
					<CoinIcon size="sm" coinType={coinType} />
				</div>
			))}
		</div>
	);
}
