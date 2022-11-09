// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ReactNode } from 'react';

export interface NavItemProps {
    onClick?(): void;
    beforeIcon?: ReactNode;
    afterIcon?: ReactNode;
    children: ReactNode;
}

export function NavItem({
    onClick,
    children,
    beforeIcon,
    afterIcon,
}: NavItemProps) {
    return (
        <button
            type="button"
            className="flex items-center gap-1 text-white text-heading6 font-medium rounded-md py-3 px-2 cursor-pointer bg-transparent border-none outline-none hover:bg-sui-grey-100 hover:bg-opacity-60"
            onClick={onClick}
        >
            {beforeIcon}
            <span>{children}</span>
            {afterIcon}
        </button>
    );
}
