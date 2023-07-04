// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatDate, formatAmountParts } from '@mysten/core';
import { format, isToday, isYesterday } from 'date-fns';
import { useMemo } from 'react';

import { Checkpoint } from '~/components/HomeMetrics/Checkpoint';
import { useEpochProgress } from '~/pages/epochs/utils';
import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';
import { ProgressBar } from '~/ui/ProgressBar';
import { Text } from '~/ui/Text';
import { LinkWithQuery } from '~/ui/utils/LinkWithQuery';
import { ampli } from '~/utils/analytics/ampli';

export function CurrentEpoch() {
	const { epoch, progress, label, end, start } = useEpochProgress();

	const formattedDateString = useMemo(() => {
		if (!start) {
			return null;
		}

		let formattedDate = '';
		const epochStartDate = new Date(start);
		if (isToday(epochStartDate)) {
			formattedDate = 'Today';
		} else if (isYesterday(epochStartDate)) {
			formattedDate = 'Yesterday';
		} else {
			formattedDate = format(epochStartDate, 'PPP');
		}
		const formattedTime = format(epochStartDate, 'p');
		return `${formattedTime}, ${formattedDate}`;
	}, [start]);

	return (
		<LinkWithQuery
			to={`/epoch/${epoch}`}
			onClick={() => ampli.clickedCurrentEpochCard({ epoch: Number(epoch) })}
		>
			<Card bg="white" height="full" spacing="lg">
				<div className="flex flex-col gap-6">
					<div className="flex w-full flex-col gap-2">
						<Heading color="success-dark" variant="heading4/semibold">
							Epoch {formatAmountParts(epoch)}
						</Heading>
						<Text variant="pSubtitle/semibold" color="steel-dark">
							{!progress && end
								? `End ${formatDate(end)}`
								: formattedDateString
								? `Started ${formattedDateString}`
								: '--'}
						</Text>
						<div className="space-y-1.5">
							<Heading variant="heading6/medium" color="steel-darker">
								{label ?? '--'}
							</Heading>
							<ProgressBar animate progress={progress || 0} />
						</div>
					</div>
					<Checkpoint />
				</div>
			</Card>
		</LinkWithQuery>
	);
}
