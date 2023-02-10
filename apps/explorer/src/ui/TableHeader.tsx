// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ReactNode } from 'react';

import { Heading, type HeadingProps } from './Heading';

export interface TableHeaderProps extends Pick<HeadingProps, 'as'> {
    children: ReactNode;
    after?: ReactNode;
}

export function TableHeader({ as = 'h3', children, after }: TableHeaderProps) {
    return (
        <div className="flex items-center border-b border-solid border-gray-45 pb-5">
            <div className="flex-1">
                <Heading as={as} variant="heading4/semibold" color="gray-90">
                    {children}
                </Heading>
            </div>
            {after && <div className="flex items-center">{after}</div>}
        </div>
    );
}
