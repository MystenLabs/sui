// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CheckFill16 } from '@mysten/icons';
import { cx } from 'class-variance-authority';

import { useMiddleEllipsis } from '../hooks';
import { Text } from '../shared/text';

export type WalletListSelectItemProps = {
    address: string;
    selected: boolean;
};

export function WalletListSelectItem({
    address,
    selected,
}: WalletListSelectItemProps) {
    const addressShort = useMiddleEllipsis(address);
    return (
        <div
            className={cx(
                'transition flex flex-row flex-nowrap items-center gap-3 py-2 cursor-pointer',
                'hover:text-steel-dark',
                selected ? 'text-steel-dark' : 'text-steel'
            )}
        >
            <CheckFill16
                className={cx(
                    selected ? 'text-success' : 'text-gray-50',
                    'transition text-base font-bold'
                )}
            />
            <Text mono variant="body" weight="semibold">
                {addressShort}
            </Text>
        </div>
    );
}
