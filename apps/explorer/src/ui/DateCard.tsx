// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatDate } from '@mysten/core';
import { Text } from '@mysten/ui';

export interface DateCardProps {
	date: Date | number;
}

// TODO - add format options
export function DateCard({ date }: DateCardProps) {
	const dateStr = formatDate(date, ['month', 'day', 'year', 'hour', 'minute']);

	if (!dateStr) {
		return null;
	}

	return (
		<Text variant="bodySmall/semibold" color="steel-dark">
			<time dateTime={new Date(date).toISOString()}>{dateStr}</time>
		</Text>
	);
}
