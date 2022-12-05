// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAddress } from '../utils/stringUtils';

import { Link } from '~/ui/Link';

export type AddressLinkProps = {
    address: string;
    noTruncate?: boolean;
};

export type ObjectLinkProps = {
    objectId: string;
    noTruncate?: boolean;
};

export function AddressLink({ address, noTruncate }: AddressLinkProps) {
    const truncatedAddress = noTruncate ? address : formatAddress(address);
    return (
        <Link variant="mono" to={`/address/${encodeURIComponent(address)}`}>
            {truncatedAddress}
        </Link>
    );
}

export function ObjectLink({ objectId, noTruncate }: ObjectLinkProps) {
    const truncatedObjectId = noTruncate ? objectId : formatAddress(objectId);
    return (
        <Link variant="mono" to={`/object/${encodeURIComponent(objectId)}`}>
            {truncatedObjectId}
        </Link>
    );
}
