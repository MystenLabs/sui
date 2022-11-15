// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Heading } from '~/ui/Heading';

const SIZE_FORMAT = {
    lg: 'heading2',
    md: 'heading6',
} as const;

export type AmountProps = {
    amount: number | string;
    symbol?: string | null;
    size?: 'lg' | 'md';
};

export function Amount({ amount, symbol, size = 'md' }: AmountProps) {
    const isLarge = size === 'lg';
    return (
        <div className="flex items-end gap-1 text-sui-grey-100 break-words">
            <Heading
                variant={SIZE_FORMAT[size]}
                weight={isLarge ? 'bold' : 'semibold'}
            >
                {amount}
            </Heading>
            {symbol && (
                <div className="text-sui-grey-80 text-bodySmall font-medium leading-4">
                    {isLarge ? (
                        <sup className="text-bodySmall">{symbol}</sup>
                    ) : (
                        symbol
                    )}
                </div>
            )}
        </div>
    );
}
