// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAddress } from '@mysten/sui.js';

import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';

type TxnAddressLinkProps = {
    address: string;
};

export function TxnAddressLink({ address }: TxnAddressLinkProps) {
    return (
        <ExplorerLink
            type={ExplorerLinkType.address}
            address={address}
            title="View on Sui Explorer"
            showIcon={false}
        >
            {formatAddress(address)}
        </ExplorerLink>
    );
}
