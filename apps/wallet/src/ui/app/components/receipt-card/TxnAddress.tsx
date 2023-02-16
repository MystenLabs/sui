// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAddress } from '@mysten/sui.js';

import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import { Text } from '_src/ui/app/shared/text';

type TxnAddressProps = {
    address: string;
    label: string;
};

export function TxnAddress({ address, label }: TxnAddressProps) {
    const truncatedAddress = formatAddress(address);
    return (
        <div className="flex justify-between w-full items-center py-3.5 first:pt-0">
            <Text variant="body" weight="medium" color="steel-darker">
                {label}
            </Text>

            <div className="flex gap-1 items-center">
                <ExplorerLink
                    type={ExplorerLinkType.address}
                    address={address}
                    title="View on Sui Explorer"
                    className="text-sui-dark font-mono text-body font-medium no-underline tracking-wider"
                    showIcon={false}
                >
                    {truncatedAddress}
                </ExplorerLink>
            </div>
        </div>
    );
}
