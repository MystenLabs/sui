// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
	type BalanceChangeSummary,
	CoinFormat,
	useFormatCoin,
	useCoinMetadata,
	type BalanceChange,
	useResolveSuiNSName,
	getRecognizedUnRecognizedTokenChanges,
} from '@mysten/core';
import { Heading, Text } from '@mysten/ui';
import clsx from 'clsx';
import { useMemo } from 'react';

import { Banner } from '~/ui/Banner';
import { Coin } from '~/ui/CoinsStack';
import { AddressLink } from '~/ui/InternalLink';
import { CollapsibleCard } from '~/ui/collapsible/CollapsibleCard';
import { CollapsibleSection } from '~/ui/collapsible/CollapsibleSection';

interface BalanceChangesProps {
	changes: BalanceChangeSummary;
}

function BalanceChangeEntry({ change }: { change: BalanceChange }) {
	const { amount, coinType, recipient, unRecognizedToken } = change;
	const [formatted, symbol] = useFormatCoin(amount, coinType, CoinFormat.FULL);
	const { data: coinMetaData } = useCoinMetadata(coinType);
	const isPositive = BigInt(amount) > 0n;

	if (!change) {
		return null;
	}

	return (
		<div className="flex flex-col gap-2 py-3 first:pt-0 only:pb-0 only:pt-0">
			<div className="flex justify-between gap-1">
				<div className="flex gap-2">
					<div className="w-5">
						<Coin type={coinType} />
					</div>
					<div className="flex flex-wrap gap-2 gap-y-1">
						<Text variant="pBody/semibold" color="steel-darker">
							{coinMetaData?.name || symbol}
						</Text>
						{unRecognizedToken && (
							<Banner variant="warning" icon={null} border spacing="sm">
								<div className="max-w-[70px] overflow-hidden truncate whitespace-nowrap text-captionSmallExtra font-medium uppercase leading-3 tracking-wider lg:max-w-full">
									Unrecognized
								</div>
							</Banner>
						)}
					</div>
				</div>

				<div className="flex justify-end text-right">
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
	);
}

function BalanceChangeCard({ changes, owner }: { changes: BalanceChange[]; owner: string }) {
	const { data: suinsDomainName } = useResolveSuiNSName(owner);
	const { recognizedTokenChanges, unRecognizedTokenChanges } = useMemo(
		() => getRecognizedUnRecognizedTokenChanges(changes),
		[changes],
	);

	return (
		<CollapsibleCard
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
					<CollapsibleSection key={index + change.coinType}>
						<BalanceChangeEntry change={change} />
					</CollapsibleSection>
				))}
				{unRecognizedTokenChanges.length > 0 && (
					<div
						className={clsx(
							'flex flex-col gap-2',
							recognizedTokenChanges?.length && 'border-t border-gray-45 pt-2',
						)}
					>
						{unRecognizedTokenChanges.map((change, index) => (
							<CollapsibleSection key={index + change.coinType}>
								<BalanceChangeEntry change={change} />
							</CollapsibleSection>
						))}
					</div>
				)}
			</div>
		</CollapsibleCard>
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
