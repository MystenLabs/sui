// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CheckFill16, XFill16 } from '@mysten/icons';
import { formatAddress } from '@mysten/sui.js';
import { cva, cx, type VariantProps } from 'class-variance-authority';

import { Text } from '../shared/text';

const styles = cva(
    'transition flex flex-row flex-nowrap items-center gap-3 py-2 cursor-pointer',
    {
        variants: {
            selected: {
                true: '',
                false: '',
            },
            mode: {
                select: '',
                disconnect: '',
            },
            disabled: {
                true: '',
                false: '',
            },
        },
        compoundVariants: [
            {
                mode: 'select',
                disabled: false,
                className: 'hover:text-steel-dark',
            },
            { mode: 'select', selected: true, className: 'text-steel-dark' },
            { mode: 'select', selected: false, className: 'text-steel' },
            {
                mode: 'disconnect',
                selected: true,
                className: 'text-issue-dark',
            },
            {
                mode: 'disconnect',
                selected: false,
                className: 'text-steel-dark',
            },
        ],
    }
);
type StyleProps = VariantProps<typeof styles>;
export interface WalletListSelectItemProps
    extends Omit<StyleProps, 'mode' | 'selected'> {
    selected: NonNullable<StyleProps['selected']>;
    mode: NonNullable<StyleProps['mode']>;
    address: string;
}

export function WalletListSelectItem({
    address,
    selected,
    mode,
    disabled = false,
}: WalletListSelectItemProps) {
    return (
        <div className={styles({ selected, mode, disabled })}>
            {mode === 'select' ? (
                <CheckFill16
                    className={cx(
                        selected ? 'text-success' : 'text-gray-50',
                        'transition text-base font-bold'
                    )}
                />
            ) : null}
            {mode === 'disconnect' && selected ? (
                <XFill16 className="text-issue-dark text-base font-bold" />
            ) : null}
            <Text mono variant="body" weight="semibold">
                {formatAddress(address)}
            </Text>
            {mode === 'disconnect' && !selected ? (
                <div className="flex flex-1 justify-end text-issue-dark">
                    <Text variant="subtitle" weight="normal">
                        Disconnect
                    </Text>
                </div>
            ) : null}
        </div>
    );
}
