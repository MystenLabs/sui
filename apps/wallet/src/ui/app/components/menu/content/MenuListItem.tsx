// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ChevronRight16 } from '@mysten/icons';
import { type ReactNode } from 'react';
import { Link } from 'react-router-dom';

export type ItemProps = {
    icon: ReactNode;
    title: ReactNode;
    subtitle?: ReactNode;
    to: string;
};

function MenuListItem({ icon, title, subtitle, to }: ItemProps) {
    return (
        <Link
            to={to}
            className="flex flex-nowrap items-center px-1 py-3 gap-5 no-underline overflow-hidden"
        >
            <div className="flex flex-nowrap flex-1 gap-2 items-center overflow-hidden basis-3/5">
                <div className="flex text-steel text-2xl flex-none">{icon}</div>
                <div className="flex-1 text-gray-90 text-body font-semibold truncate">
                    {title}
                </div>
            </div>
            <div className="flex flex-nowrap flex-1 justify-end gap-1 items-center overflow-hidden basis-2/5">
                {subtitle ? (
                    <div className="truncate text-steel-dark text-bodySmall font-medium">
                        {subtitle}
                    </div>
                ) : null}
                <ChevronRight16 className="text-steel flex-none" />
            </div>
        </Link>
    );
}

export default MenuListItem;
