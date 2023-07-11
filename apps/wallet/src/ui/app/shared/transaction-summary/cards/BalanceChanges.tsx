// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
	type BalanceChangeSummary,
	CoinFormat,
	useFormatCoin,
	type BalanceChange,
} from '@mysten/core';

import { CoinsStack } from './CoinStack';
import { Card } from '../Card';
import { OwnerFooter } from '../OwnerFooter';
import { Text } from '_src/ui/app/shared/text';

interface BalanceChangesProps {
	changes?: BalanceChangeSummary;
}

function BalanceChangeEntry({ change }: { change: BalanceChange }) {
	const { amount, coinType } = change;
	const isPositive = BigInt(amount) > 0n;

	const [formatted, symbol] = useFormatCoin(amount, coinType, CoinFormat.FULL);

	return (
		<div className="flex flex-col gap-2">
			<div className="flex flex-col gap-2">
				<div className="flex justify-between">
					<Text variant="pBody" weight="medium" color="steel-dark">
						Amount
					</Text>
					<div className="flex">
						<Text
							variant="pBody"
							weight="medium"
							color={isPositive ? 'success-dark' : 'issue-dark'}
						>
							{isPositive ? '+' : ''}
							{formatted} {symbol}
						</Text>
					</div>
				</div>
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
						{changes.map((change) => (
							<BalanceChangeEntry change={change} key={change.coinType + change.amount} />
						))}
					</div>
				</Card>
			))}
		</>
	);
}
