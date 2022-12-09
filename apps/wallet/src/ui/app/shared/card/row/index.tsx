// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { memo } from 'react';

import type { ReactNode } from 'react';

export type CardRowProps = {
    children: ReactNode;
};

function CardRow({ children }: CardRowProps) {
    return (
        <div className="divide-x flex divide-solid divide-gray-45 divide-y-0">
            {children}
        </div>
    );
}

export default memo(CardRow);
