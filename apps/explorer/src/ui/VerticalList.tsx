// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import clsx from 'clsx';
import { type ReactNode } from 'react';

export interface ListItemProps {
    active?: boolean;
    children: ReactNode;
    onClick?(): void;
}

export function ListItem({ active, children, onClick }: ListItemProps) {
    return (
        <li className="list-none">
            <button
                type="button"
                className={clsx(
                    'cursor-pointer py-2 rounded-md text-body block w-full text-left px-2.5 border',
                    active
                        ? 'bg-gray-45 text-gray-90 font-semibold border-solid border-gray-50 shadow-sm'
                        : 'bg-white text-gray-80 font-medium border-transparent'
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
