// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '@mysten/ui';
import { type ReactNode } from 'react';

import { DescriptionItem } from '~/ui/DescriptionList';
import { Link } from '~/ui/Link';
import { getDisplayUrl } from '~/utils/objectUtils';

export type LinkOrTextDescriptionItemProps = {
	title: ReactNode;
	value: ReactNode;
	parseUrl?: boolean;
};

export function LinkOrTextDescriptionItem({
	title,
	value,
	parseUrl = false,
}: LinkOrTextDescriptionItemProps) {
	let urlData = null;
	if (parseUrl && typeof value === 'string') {
		urlData = getDisplayUrl(value);
	}
	return value ? (
		<DescriptionItem title={title}>
			{urlData && typeof urlData === 'object' ? (
				<Link href={urlData.href} variant="textHeroDark">
					{urlData.display}
				</Link>
			) : typeof value === 'string' ? (
				<Text variant="pBody/medium" color="steel-darker">
					{value}
				</Text>
			) : (
				value
			)}
		</DescriptionItem>
	) : null;
}
