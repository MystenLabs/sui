// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { AddressLink } from './InternalLink';

export type TransactionAddressProps = {
    address: string;
    icon: React.ReactNode;
};

export function TransactionAddress({ icon, address }: TransactionAddressProps) {
    return (
        <div className="flex items-center gap-2 break-all">
            <div className="w-4">{icon}</div>
            <AddressLink address={address} size="md" />
        </div>
    );
}
