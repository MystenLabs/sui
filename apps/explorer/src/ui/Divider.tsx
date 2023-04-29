// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import clsx from 'clsx';

export interface DividerProps {
    vertical?: boolean;
}

export function Divider({ vertical }: DividerProps) {
    return (
        <div
            className={clsx(
                'border-gray-45',
                vertical ? 'border-l' : 'grow border-b'
            )}
        />
    );
}
