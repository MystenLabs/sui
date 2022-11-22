// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { truncate } from '../utils/stringUtils';

import { Link } from '~/ui/Link';

const TRUNCATE_LENGTH = 16;

export type AddressLinkProps = {
    address: string;
    noTruncate?: boolean;
};

export type ObjectLinkProps = {
    objectId: string;
    noTruncate?: boolean;
};

export function AddressLink({ address, noTruncate }: AddressLinkProps) {
    const truncatedAddress = noTruncate
        ? address
        : truncate(address, TRUNCATE_LENGTH);
    return (
        <Link variant="mono" to={`/address/${encodeURIComponent(address)}`}>
            {truncatedAddress}
        </Link>
    );
}

export function ObjectLink({ objectId, noTruncate }: ObjectLinkProps) {
    const truncatedObjectId = noTruncate
        ? objectId
        : truncate(objectId, TRUNCATE_LENGTH);
    return (
        <Link variant="mono" to={`/objects/${encodeURIComponent(objectId)}`}>
            {truncatedObjectId}
        </Link>
    );
}
