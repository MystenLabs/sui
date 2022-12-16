// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAddress } from '../utils/stringUtils';

import { Link, type LinkProps } from '~/ui/Link';

export interface AddressLinkProps extends LinkProps {
    address: string;
    noTruncate?: boolean;
}

export interface ObjectLinkProps extends LinkProps {
    objectId: string;
    noTruncate?: boolean;
}

export interface TransactionLinkProps extends LinkProps {
    digest: string;
    noTruncate?: boolean;
}

export function AddressLink({
    address,
    noTruncate,
    ...props
}: AddressLinkProps) {
    const truncatedAddress = noTruncate ? address : formatAddress(address);
    return (
        <Link
            variant="mono"
            to={`/address/${encodeURIComponent(address)}`}
            {...props}
        >
            {truncatedAddress}
        </Link>
    );
}

export function ObjectLink({
    objectId,
    noTruncate,
    ...props
}: ObjectLinkProps) {
    const truncatedObjectId = noTruncate ? objectId : formatAddress(objectId);
    return (
        <Link
            variant="mono"
            to={`/object/${encodeURIComponent(objectId)}`}
            {...props}
        >
            {truncatedObjectId}
        </Link>
    );
}

export function TransactionLink({
    digest,
    noTruncate,
    ...props
}: TransactionLinkProps) {
    const truncatedDigest = noTruncate ? digest : formatAddress(digest);
    return (
        <Link
            variant="mono"
            to={`/transaction/${encodeURIComponent(digest)}`}
            {...props}
        >
            {truncatedDigest}
        </Link>
    );
}
