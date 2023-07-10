// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useTimeAgo } from '@mysten/core';

type Prop = {
	timestamp: number | undefined;
	renderAsTimestamp?: boolean;
};

export function TxTimeType({ timestamp, renderAsTimestamp }: Prop) {
	const timeAgo = useTimeAgo({
		timeFrom: timestamp || null,
		shortedTimeLabel: true,
	});

	const formattedTimestamp = new Date(timestamp || 0).toLocaleString('en-US', {
		month: 'short',
		day: 'numeric',
		hour: 'numeric',
		minute: 'numeric',
		second: 'numeric',
		hour12: true,
	});

	return (
		<section>
			<div className="w-20 text-caption">{renderAsTimestamp ? formattedTimestamp : timeAgo}</div>
		</section>
	);
}
