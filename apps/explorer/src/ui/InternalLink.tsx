// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiNSName } from '@mysten/core';
import { formatAddress, formatDigest } from '@mysten/sui.js';

import { Link, type LinkProps } from '~/ui/Link';

interface BaseInternalLinkProps extends LinkProps {
	noTruncate?: boolean;
	label?: string;
	queryStrings?: Record<string, string>;
}

function createInternalLink<T extends string>(
	base: string,
	propName: T,
	formatter: (id: string) => string = (id) => id,
) {
	return ({
		[propName]: id,
		noTruncate,
		label,
		queryStrings = {},
		...props
	}: BaseInternalLinkProps & Record<T, string>) => {
		const truncatedAddress = noTruncate ? id : formatter(id);
		const queryString = new URLSearchParams(queryStrings).toString();
		const queryStringPrefix = queryString ? `?${queryString}` : '';

		return (
			<Link variant="mono" to={`/${base}/${encodeURI(id)}${queryStringPrefix}`} {...props}>
				{label || truncatedAddress}
			</Link>
		);
	};
}

export const EpochLink = createInternalLink('epoch', 'epoch');
export const CheckpointLink = createInternalLink('checkpoint', 'digest', formatAddress);
export const CheckpointSequenceLink = createInternalLink('checkpoint', 'sequence');
export const AddressLink = createInternalLink('address', 'address', (addressOrNs) => {
	if (isSuiNSName(addressOrNs)) {
		return addressOrNs;
	}
	return formatAddress(addressOrNs);
});
export const ObjectLink = createInternalLink('object', 'objectId', formatAddress);
export const TransactionLink = createInternalLink('txblock', 'digest', formatDigest);
export const ValidatorLink = createInternalLink('validator', 'address', formatAddress);
