// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import Alert from '_components/alert';
import { CoinIcon } from '_src/ui/app/components/coin-icon';
import { Text } from '_src/ui/app/shared/text';
import {
	CoinFormat,
	getRecognizedUnRecognizedTokenChanges,
	useCoinMetadata,
	useFormatCoin,
	type BalanceChange,
	type BalanceChangeSummary,
} from '@mysten/core';
import classNames from 'clsx';
import { useMemo } from 'react';

import { Card } from '../Card';
import { OwnerFooter } from '../OwnerFooter';

interface BalanceChangesProps {
	changes?: BalanceChangeSummary;
}

function BalanceChangeEntry({ change }: { change: BalanceChange }) {
	const { amount, coinType, unRecognizedToken } = change;
	const isPositive = BigInt(amount) > 0n;
	const [formatted, symbol] = useFormatCoin(amount, coinType, CoinFormat.FULL);
	const { data: coinMetaData } = useCoinMetadata(coinType);
	return (
		<div className="flex flex-col gap-2">
			<div className="flex justify-between">
				<div className="flex items-center gap-2">
					<div className="w-5">
						<CoinIcon size="sm" coinType={coinType} />
					</div>
					<div className="flex flex-wrap gap-2 gap-y-1 truncate">
						<Text variant="pBody" weight="semibold" color="steel-darker">
							{coinMetaData?.name || symbol}
						</Text>
						{unRecognizedToken && (
							<Alert mode="warning" spacing="sm" showIcon={false}>
								<div className="item-center leading-none max-w-[70px] overflow-hidden truncate whitespace-nowrap text-captionSmallExtra font-medium uppercase tracking-wider">
									Unrecognized
								</div>
							</Alert>
						)}
					</div>
				</div>
				<div className="flex justify-end w-full text-right">
					<Text variant="pBody" weight="medium" color={isPositive ? 'success-dark' : 'issue-dark'}>
						{isPositive ? '+' : ''}
						{formatted} {symbol}
					</Text>
				</div>
			</div>
		</div>
	);
}

function BalanceChangeEntries({ changes }: { changes: BalanceChange[] }) {
	const { recognizedTokenChanges, unRecognizedTokenChanges } = useMemo(
		() => getRecognizedUnRecognizedTokenChanges(changes),
		[changes],
	);

	return (
		<div className="flex flex-col gap-2">
			<div className="flex flex-col gap-4 pb-3">
				{recognizedTokenChanges.map((change) => (
					<BalanceChangeEntry change={change} key={change.coinType + change.amount} />
				))}
				{unRecognizedTokenChanges.length > 0 && (
					<div
						className={classNames(
							'flex flex-col gap-2 pt-2',
							recognizedTokenChanges?.length && 'border-t border-gray-45',
						)}
					>
						{unRecognizedTokenChanges.map((change, index) => (
							<BalanceChangeEntry change={change} key={change.coinType + index} />
						))}
					</div>
				)}
			</div>
		</div>
	);
}

export function BalanceChanges({ changes }: BalanceChangesProps) {
	if (!changes) return null;
	return (
		<>
			{Object.entries(changes).map(([owner, changes]) => (
				<Card heading="Balance Changes" key={owner} footer={<OwnerFooter owner={owner} />}>
					<div className="flex flex-col gap-4 pb-3">
						<BalanceChangeEntries changes={changes} />
					</div>
				</Card>
			))}
		</>
	);
}
