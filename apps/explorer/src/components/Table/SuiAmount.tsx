// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CoinFormat, useFormatCoin } from '@mysten/core';
import { SUI_TYPE_ARG } from '@mysten/sui.js';
import { Text } from '@mysten/ui';

export function SuiAmount({
	amount,
	full = false,
}: {
	amount?: bigint | number | string | null;
	full?: boolean;
}) {
	const [formattedAmount, coinType] = useFormatCoin(
		amount,
		SUI_TYPE_ARG,
		full ? CoinFormat.FULL : CoinFormat.ROUNDED,
	);
	if (!amount) return <Text variant="bodySmall/medium">--</Text>;

	return (
		<div className="leading-1 flex items-end gap-0.5">
			<Text variant="bodySmall/medium" color="steel-darker">
				{formattedAmount}
			</Text>
			<Text variant="captionSmall/normal" color="steel-dark">
				{coinType}
			</Text>
		</div>
	);
}
