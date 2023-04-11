// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAddress, formatDigest } from '@mysten/sui.js';

import { Link, type LinkProps } from '~/ui/Link';

interface BaseInternalLinkProps extends LinkProps {
    noTruncate?: boolean;
    label?: string;
}

function createInternalLink<T extends string>(
    base: string,
    propName: T,
    formatter = formatAddress
) {
    return ({
        [propName]: id,
        noTruncate,
        label,
        ...props
    }: BaseInternalLinkProps & Record<T, string>) => {
        const truncatedAddress = noTruncate ? id : formatter(id);
        return (
            <Link variant="mono" to={`/${base}/${encodeURI(id)}`} {...props}>
                {label || truncatedAddress}
            </Link>
        );
    };
}

export const EpochLink = createInternalLink('epoch', 'epoch', (epoch) => epoch);
export const CheckpointLink = createInternalLink('checkpoint', 'digest');
export const CheckpointSequenceLink = createInternalLink(
    'checkpoint',
    'sequence',
    (sequence: string) => sequence
);
export const AddressLink = createInternalLink('address', 'address');
export const ObjectLink = createInternalLink('object', 'objectId');
export const TransactionLink = createInternalLink(
    'txblock',
    'digest',
    formatDigest
);
export const ValidatorLink = createInternalLink('validator', 'address');
