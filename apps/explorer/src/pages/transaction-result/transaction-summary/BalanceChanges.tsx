// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
	type BalanceChangeSummary,
	CoinFormat,
	useFormatCoin,
	type BalanceChange,
	useResolveSuiNSName,
	useCoinMetadata,
} from '@mysten/core';
import { parseStructTag, normalizeSuiObjectId } from '@mysten/sui.js';
import { Heading, Text } from '@mysten/ui';
import { useMemo } from 'react';

import { useRecognizedPackages } from '~/hooks/useRecognizedPackages';
import { Banner } from '~/ui/Banner';
import { Coin } from '~/ui/CoinsStack';
import { AddressLink } from '~/ui/InternalLink';
import { TransactionBlockCard, TransactionBlockCardSection } from '~/ui/TransactionBlockCard';

interface BalanceChangesProps {
	changes: BalanceChangeSummary;
}

function BalanceChangeEntry({
	change,
	notRecognizedToken,
}: {
	change: BalanceChange;
	notRecognizedToken?: boolean;
}) {
	const { amount, coinType, recipient } = change;
	const { data: coinMetadata } = useCoinMetadata(coinType);
	const [formatted, symbol] = useFormatCoin(amount, coinType, CoinFormat.FULL);
	const isPositive = BigInt(amount) > 0n;

	if (!change) {
		return null;
	}

	return (
		<div className="flex flex-col gap-2 py-3 first:pt-0 only:pb-0 only:pt-0">
			<div className="flex flex-col gap-2">
				<div className="flex flex-wrap justify-between gap-2">
					<div className="flex gap-2">
						<Coin type={coinType} />
						<div className="flex flex-col  gap-2 gap-y-1 lg:flex-row">
							<Text variant="pBody/semibold" color="steel-darker" truncate>
								{coinMetadata?.name || coinMetadata?.symbol}
							</Text>
							{notRecognizedToken && (
								<Banner variant="warning" icon={null} border spacing="sm">
									<div className="break-normal text-captionSmallExtra uppercase tracking-wider">
										Unrecognized
									</div>
								</Banner>
							)}
						</div>
					</div>

					<div className="flex">
						<Text variant="pBody/medium" color={isPositive ? 'success-dark' : 'issue-dark'}>
							{isPositive ? '+' : ''}
							{formatted} {symbol}
						</Text>
					</div>
				</div>

				{recipient && (
					<div className="flex flex-wrap items-center justify-between border-t border-gray-45 pt-2">
						<Text variant="pBody/medium" color="steel-dark">
							Recipient
						</Text>
						<AddressLink address={recipient} />
					</div>
				)}
			</div>
		</div>
	);
}

function BalanceChangeCard({ changes, owner }: { changes: BalanceChange[]; owner: string }) {
	const { data: suinsDomainName } = useResolveSuiNSName(owner);
	const recognizedPackagesList = useRecognizedPackages();

	const normalizedRecognizedPackages = useMemo(
		() => recognizedPackagesList.map(normalizeSuiObjectId) as string[],
		[recognizedPackagesList],
	);
	const { recognizedTokenChanges, notRecognizedTokenChanges } = useMemo(() => {
		const recognizedTokenChanges = [];
		const notRecognizedTokenChanges = [];
		for (let change of changes) {
			const { address: packageId } = parseStructTag(change.coinType);
			if (normalizedRecognizedPackages.includes(packageId)) {
				recognizedTokenChanges.push(change);
			} else {
				notRecognizedTokenChanges.push(change);
			}
		}
		return { recognizedTokenChanges, notRecognizedTokenChanges };
	}, [changes, normalizedRecognizedPackages]);

	return (
		<TransactionBlockCard
			title={
				<div className="flex w-full flex-wrap items-center justify-between gap-y-2">
					<Heading variant="heading6/semibold" color="steel-darker">
						Balance Changes
					</Heading>
				</div>
			}
			shadow
			size="sm"
			footer={
				owner ? (
					<div className="flex flex-wrap justify-between">
						<Text variant="pBody/medium" color="steel-dark">
							Owner
						</Text>
						<Text variant="pBody/medium" color="hero-dark">
							<AddressLink label={suinsDomainName || undefined} address={owner} />
						</Text>
					</div>
				) : null
			}
		>
			<div className="flex flex-col gap-2">
				{recognizedTokenChanges.map((change, index) => (
					<TransactionBlockCardSection key={index + change.coinType}>
						<BalanceChangeEntry change={change} />
					</TransactionBlockCardSection>
				))}
				{notRecognizedTokenChanges.length > 0 && (
					<div className="flex flex-col gap-2">
						<div className="flex border-t border-gray-45 pt-2">
							<Text variant="pSubtitleSmall/medium" color="steel-dark">
								Coins below are not recognized by <span className="text-hero">Sui Foundation.</span>
							</Text>
						</div>
						{notRecognizedTokenChanges.map((change, index) => (
							<TransactionBlockCardSection key={index + change.coinType}>
								<BalanceChangeEntry change={change} notRecognizedToken />
							</TransactionBlockCardSection>
						))}
					</div>
				)}
			</div>
		</TransactionBlockCard>
	);
}

export function BalanceChanges({ changes }: BalanceChangesProps) {
	if (!changes) return null;

	return (
		<>
			{Object.entries(changes).map(([owner, changes]) => (
				<BalanceChangeCard key={owner} changes={changes} owner={owner} />
			))}
		</>
	);
}
