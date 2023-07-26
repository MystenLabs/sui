// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { formatDate, useResolveSuiNSName } from '@mysten/core';
import { Heading, Text } from '@mysten/ui';
import { type ReactNode } from 'react';

import { useBreakpoint } from '~/hooks/useBreakpoint';
import { AddressLink, CheckpointSequenceLink, EpochLink } from '~/ui/InternalLink';
import { TransactionBlockCard, TransactionBlockCardSection } from '~/ui/TransactionBlockCard';

export function TransactionDetail({ label, value }: { label: string; value: ReactNode | string }) {
	return (
		<div className="flex basis-1/3 flex-col gap-2 pl-3 first:pl-0 md:pl-5">
			<Heading variant="heading4/semibold" color="steel-darker">
				{label}
			</Heading>
			<Text variant="pBody/normal" color="steel-dark">
				{value}
			</Text>
		</div>
	);
}

interface TransactionDetailsProps {
	sender?: string;
	checkpoint?: string;
	executedEpoch?: string;
	timestamp?: string;
}

export function TransactionDetailCard({
	sender,
	checkpoint,
	executedEpoch,
	timestamp,
}: TransactionDetailsProps) {
	const md = useBreakpoint('md');
	const { data: domainName } = useResolveSuiNSName(sender);

	return (
		<TransactionBlockCard size={md ? 'md' : 'sm'}>
			<TransactionBlockCardSection>
				<div className="flex flex-col gap-6">
					{timestamp && (
						<Text variant="pBody/medium" color="steel-dark">
							{formatDate(Number(timestamp))}
						</Text>
					)}
					<div className="flex justify-between gap-3 divide-x divide-gray-45 md:gap-5">
						{sender && (
							<TransactionDetail
								label="Sender"
								value={<AddressLink address={domainName ?? sender} />}
							/>
						)}
						{checkpoint && (
							<TransactionDetail
								label="Checkpoint"
								value={
									<CheckpointSequenceLink
										sequence={checkpoint}
										label={Number(checkpoint).toLocaleString()}
									/>
								}
							/>
						)}
						{executedEpoch && (
							<TransactionDetail label="Epoch" value={<EpochLink epoch={executedEpoch} />} />
						)}
					</div>
				</div>
			</TransactionBlockCardSection>
		</TransactionBlockCard>
	);
}
