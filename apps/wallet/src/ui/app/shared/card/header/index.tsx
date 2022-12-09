// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { memo } from 'react';

import type { ReactNode } from 'react';

export type CardHeaderProps = {
    children: ReactNode | ReactNode[];
};

function CardHeader({ children }: CardHeaderProps) {
    return (
        <div className="bg-gray-40 min-h-[30px] flex justify-center items-center rounded-t-2xl divide-x divide-solid divide-gray-45 divide-y-0 w-full">
            {children}
        </div>
    );
}

export default memo(CardHeader);
