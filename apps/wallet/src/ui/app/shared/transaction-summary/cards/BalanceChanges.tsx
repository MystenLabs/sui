// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
	type BalanceChangeSummary,
	CoinFormat,
	useFormatCoin,
	useCoinMetadata,
	type BalanceChange,
} from '@mysten/core';
import { useMemo } from 'react';

import { CoinsStack } from './CoinStack';
import { Card } from '../Card';
import { OwnerFooter } from '../OwnerFooter';
import Alert from '_components/alert';
import { CoinIcon } from '_src/ui/app/components/coin-icon';
import { Text } from '_src/ui/app/shared/text';

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
				<div className="flex gap-2">
					<div className="w-5">
						<CoinIcon size="sm" coinType={coinType} />
					</div>
					<div className="flex flex-wrap gap-2 gap-y-1">
						<Text variant="pBody" weight="semibold" color="steel-darker">
							{coinMetaData?.name || symbol}
						</Text>
						{unRecognizedToken && (
							<Alert mode="warning">
								<div className="item-center max-w-[70px] overflow-hidden truncate whitespace-nowrap text-captionSmallExtra font-medium uppercase tracking-wider lg:max-w-full">
									Unrecognized
								</div>
							</Alert>
						)}
					</div>
				</div>
				<div className="flex">
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
	const { recognizedTokenChanges, unRecognizedTokenChanges } = useMemo(() => {
		const recognizedTokenChanges = [];
		const unRecognizedTokenChanges = [];
		for (let change of changes) {
			if (change.unRecognizedToken) {
				unRecognizedTokenChanges.push(change);
			} else {
				recognizedTokenChanges.push(change);
			}
		}
		return { recognizedTokenChanges, unRecognizedTokenChanges };
	}, [changes]);

	return (
		<div className="flex flex-col gap-2">
			<div className="flex flex-col gap-4 pb-3">
				{recognizedTokenChanges.map((change) => (
					<BalanceChangeEntry change={change} key={change.coinType + change.amount} />
				))}
				{unRecognizedTokenChanges.length > 0 && (
					<div className="flex flex-col gap-2 border-t border-gray-45 pt-2">
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
				<Card
					heading="Balance Changes"
					key={owner}
					after={<CoinsStack coinTypes={Array.from(new Set(changes.map((c) => c.coinType)))} />}
					footer={<OwnerFooter owner={owner} />}
				>
					<div className="flex flex-col gap-4 pb-3">
						<BalanceChangeEntries changes={changes} />
					</div>
				</Card>
			))}
		</>
	);
}
