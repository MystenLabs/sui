// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { forwardRef, type ComponentProps, type ReactNode } from 'react';

export interface NavItemProps extends ComponentProps<'button'> {
    beforeIcon?: ReactNode;
    afterIcon?: ReactNode;
    children: ReactNode;
}

export const NavItem = forwardRef<HTMLButtonElement, NavItemProps>(
    ({ children, beforeIcon, afterIcon, ...props }, ref) => {
        return (
            <button
                ref={ref}
                type="button"
                className="flex items-center gap-1 text-white text-heading6 font-medium rounded-md py-3 px-4 cursor-pointer bg-transparent border-none outline-none hover:bg-gray-100/60 ui-open:bg-gray-100/60"
                {...props}
            >
                {beforeIcon}
                {children}
                {afterIcon}
            </button>
        );
    }
);
