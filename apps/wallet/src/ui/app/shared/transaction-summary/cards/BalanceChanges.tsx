// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
	type BalanceChangeSummary,
	CoinFormat,
	useFormatCoin,
	type BalanceChange,
} from '@mysten/core';
import { parseStructTag, normalizeSuiObjectId } from '@mysten/sui.js';
import { useMemo } from 'react';

import { CoinsStack } from './CoinStack';
import { Card } from '../Card';
import { OwnerFooter } from '../OwnerFooter';
import Alert from '_components/alert';
import { CoinIcon } from '_src/ui/app/components/coin-icon';
import { useRecognizedPackages } from '_src/ui/app/hooks/useRecognizedPackages';
import { Text } from '_src/ui/app/shared/text';

interface BalanceChangesProps {
	changes?: BalanceChangeSummary;
}

function BalanceChangeEntry({
	change,
	isUnRecognizedToken,
}: {
	change: BalanceChange;
	isUnRecognizedToken?: boolean;
}) {
	const { amount, coinType } = change;
	const isPositive = BigInt(amount) > 0n;
	const [formatted, symbol, queryResult] = useFormatCoin(amount, coinType, CoinFormat.FULL);

	return (
		<div className="flex flex-col gap-2">
			<div className="flex flex-col gap-2">
				{isUnRecognizedToken && (
					<div className="border-t border-gray-45 pt-2">
						<Text variant="pSubtitleSmall" weight="normal" color="steel-dark">
							Coins below are not recognized by <span className="text-hero">Sui Foundation.</span>
						</Text>
					</div>
				)}
				<div className="flex justify-between">
					<div className="flex gap-2">
						<div className="w-5">
							<CoinIcon size="sm" coinType={coinType} />
						</div>
						<div className="flex max-w-[90px] flex-wrap gap-2 gap-y-1 sm:max-w-full">
							<Text variant="pBody" weight="semibold" color="steel-darker">
								{queryResult.data?.name || symbol}
							</Text>
							{isUnRecognizedToken && (
								<Alert mode="warning">
									<div className="item-center truncate break-normal text-captionSmallExtra font-medium uppercase tracking-wider">
										Unrecognized
									</div>
								</Alert>
							)}
						</div>
					</div>
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

function BalanceChangeEntries({ changes }: { changes: BalanceChange[] }) {
	const recognizedPackagesList = useRecognizedPackages();
	const normalizedRecognizedPackages = useMemo(
		() => recognizedPackagesList.map((itm) => normalizeSuiObjectId(itm)),
		[recognizedPackagesList],
	);
	const { recognizedTokenChanges, unRecognizedTokenChanges } = useMemo(() => {
		const recognizedTokenChanges = [];
		const unRecognizedTokenChanges = [];
		for (let change of changes) {
			const { address: packageId } = parseStructTag(change.coinType);
			if (normalizedRecognizedPackages.includes(packageId)) {
				recognizedTokenChanges.push(change);
			} else {
				unRecognizedTokenChanges.push(change);
			}
		}
		return { recognizedTokenChanges, unRecognizedTokenChanges };
	}, [changes, normalizedRecognizedPackages]);

	return (
		<div className="flex flex-col gap-2">
			<div className="flex flex-col gap-4 pb-3">
				{recognizedTokenChanges.map((change) => (
					<BalanceChangeEntry change={change} key={change.coinType + change.amount} />
				))}
				{unRecognizedTokenChanges.length > 0 && (
					<div className="flex flex-col gap-2">
						<div className="flex border-t border-gray-45 pt-2">
							<Text variant="pSubtitleSmall" weight="medium" color="steel-dark">
								Coins below are not recognized by <span className="text-hero">Sui Foundation.</span>
							</Text>
						</div>
						{unRecognizedTokenChanges.map((change, index) => (
							<BalanceChangeEntry
								change={change}
								key={change.coinType + index}
								isUnRecognizedToken
							/>
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
