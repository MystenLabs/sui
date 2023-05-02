// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import clsx from 'clsx';

export interface DividerProps {
    vertical?: boolean;
    color?: string;
}

export function Divider({ vertical, color }: DividerProps) {
    return (
        <div
            className={clsx(
                (!color || color === 'gray45') && 'border-gray-45',
                color === 'gray40' && 'border-gray-40',
                vertical ? 'border-l' : 'grow border-b'
            )}
        />
    );
}
