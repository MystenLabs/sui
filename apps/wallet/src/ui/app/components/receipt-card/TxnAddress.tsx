// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import { useMiddleEllipsis } from '_hooks';
import { Text } from '_src/ui/app/shared/text';

const TRUNCATE_MAX_LENGTH = 8;
const TRUNCATE_PREFIX_LENGTH = 4;

type TxnAddressProps = {
    address: string;
    label: string;
};

export function TxnAddress({ address, label }: TxnAddressProps) {
    const txnAddress = useMiddleEllipsis(
        address,
        TRUNCATE_MAX_LENGTH,
        TRUNCATE_PREFIX_LENGTH
    );
    return (
        <div className="flex justify-between w-full items-center pt-3.5">
            <Text variant="body" weight="medium" color="steel-darker">
                {label}
            </Text>

            <div className="flex gap-1 items-center">
                <ExplorerLink
                    type={ExplorerLinkType.address}
                    address={address}
                    title="View on Sui Explorer"
                    className="text-sui-dark font-mono text-body font-semibold no-underline uppercase tracking-wider"
                    showIcon={false}
                >
                    {txnAddress}
                </ExplorerLink>
            </div>
        </div>
    );
}
