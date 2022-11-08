// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import clsx from 'clsx';
import { type ReactNode } from 'react';

export interface ListItemProps {
    active?: boolean;
    isBlue?: boolean;
    children: ReactNode;
    onClick?(): void;
}

export function ListItem({ active, isBlue, children, onClick }: ListItemProps) {
    const activeStyle = isBlue
        ? 'bg-sui/10 text-sui-grey-80 border-transparent'
        : 'bg-sui-grey-45 text-sui-grey-90 font-semibold border-solid border-sui-grey-50 shadow-sm';

    return (
        <li className="list-none">
            <button
                type="button"
                className={clsx(
                    'cursor-pointer py-2 rounded-md text-body block w-full text-left mt-0.5 px-1.5 border',
                    active
                        ? activeStyle
                        : 'bg-white text-sui-grey-80 font-medium border-transparent'
                )}
                onClick={onClick}
            >
                {children}
            </button>
        </li>
    );
}

export interface VerticalListProps {
    children: ReactNode;
}

export function VerticalList({ children }: VerticalListProps) {
    return <ul className="flex flex-col p-0 m-0 gap-1">{children}</ul>;
}
