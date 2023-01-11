// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAddress } from '../utils/stringUtils';

import { Link, type LinkProps } from '~/ui/Link';

interface BaseInternalLinkProps extends LinkProps {
    noTruncate?: boolean;
}

function createInternalLink<T extends string>(base: string, propName: T) {
    return ({
        [propName]: id,
        noTruncate,
        ...props
    }: BaseInternalLinkProps & Record<T, string>) => {
        const truncatedAddress = noTruncate ? id : formatAddress(id);
        return (
            <Link
                variant="mono"
                to={`/${base}/${encodeURIComponent(id)}`}
                {...props}
            >
                {truncatedAddress}
            </Link>
        );
    };
}

export const AddressLink = createInternalLink('address', 'address');
export const ObjectLink = createInternalLink('object', 'objectId');
export const TransactionLink = createInternalLink('transaction', 'digest');
