// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '_app/shared/text';
import { CoinIcon } from '_components/coin-icon';
import { useFormatCoin } from '@mysten/core';
import { type ReactNode } from 'react';

type CoinItemProps = {
	coinType: string;
	balance: bigint;
	isActive?: boolean;
	usd?: number;
	centerAction?: ReactNode;
	subtitle?: string;
};

export function CoinItem({
	coinType,
	balance,
	isActive,
	usd,
	centerAction,
	subtitle,
}: CoinItemProps) {
	const [formatted, symbol, { data: coinMeta }] = useFormatCoin(balance, coinType);

	return (
		<div className="flex gap-2.5 w-full py-3 pl-1.5 pr-2 justify-center items-center rounded hover:bg-sui/10">
			<CoinIcon coinType={coinType} size={isActive ? 'sm' : 'md'} />
			<div className="flex flex-1 gap-1.5 justify-between items-center">
				<div className="max-w-token-width">
					<Text variant="body" color="gray-90" weight="semibold" truncate>
						{coinMeta?.name || symbol} {isActive ? 'available' : ''}
					</Text>
					{!isActive && !subtitle ? (
						<div className="mt-1.5">
							<Text variant="subtitle" color="steel-dark" weight="medium">
								{symbol}
							</Text>
						</div>
					) : null}
					{subtitle ? (
						<div className="mt-1.5">
							<Text variant="subtitle" color="steel" weight="medium">
								{subtitle}
							</Text>
						</div>
					) : null}
				</div>

				{centerAction}

				<div className="flex flex-row justify-center items-center">
					{isActive ? (
						<Text variant="body" color="steel-darker" weight="medium">
							{formatted}
						</Text>
					) : (
						<div data-testid={coinType} className="max-w-token-width">
							<Text variant="body" color="gray-90" weight="medium" truncate>
								{formatted} {symbol}
							</Text>
							{usd && (
								<Text variant="caption" color="steel-dark" weight="medium">
									${usd.toLocaleString('en-US')}
								</Text>
							)}
						</div>
					)}
				</div>
			</div>
		</div>
	);
}
