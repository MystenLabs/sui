// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useResolveSuiNSName } from '@mysten/core';

import { AddressLink } from './InternalLink';

export type TransactionAddressProps = {
    address: string;
    icon: React.ReactNode;
};

export function TransactionAddress({ icon, address }: TransactionAddressProps) {
    const { data: domainName } = useResolveSuiNSName(address);

    return (
        <div className="flex items-center gap-2 break-all">
            <div className="w-4">{icon}</div>
            <AddressLink address={domainName || address} size="md" />
        </div>
    );
}
