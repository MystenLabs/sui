// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Check12, Copy12 } from '@mysten/icons';

import { useMiddleEllipsis } from '../hooks';
import { useAccounts } from '../hooks/useAccounts';
import { useActiveAddress } from '../hooks/useActiveAddress';
import { useCopyToClipboard } from '../hooks/useCopyToClipboard';
import { Text } from '../shared/text';

import type { SuiAddress } from '@mysten/sui.js/src';

export type AccountItemProps = {
    address: SuiAddress;
    onAccountSelected: (address: SuiAddress) => void;
};

export function AccountListItem({
    address,
    onAccountSelected,
}: AccountItemProps) {
    const account = useAccounts([address])[0];
    const activeAddress = useActiveAddress();
    const addressShort = useMiddleEllipsis(address);
    const copy = useCopyToClipboard(address, {
        copySuccessMessage: 'Address Copied',
    });
    if (!account) {
        return null;
    }
    return (
        <div
            className="flex p-2.5 items-start gap-2.5 rounded-md hover:bg-sui/10 cursor-pointer focus-visible:ring-1 group transition-colors"
            onClick={() => {
                onAccountSelected(address);
            }}
        >
            <div className="flex-1">
                <Text color="steel-darker" variant="bodySmall" mono>
                    {addressShort}
                </Text>
            </div>
            {activeAddress === address ? (
                <Check12 className="text-success" />
            ) : null}
            <Copy12
                className="text-gray-60 group-hover:text-steel transition-colors hover:!text-hero-dark"
                onClick={copy}
            />
        </div>
    );
}
