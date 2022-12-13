// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';

import type { ReactNode } from 'react';

export type CardProps = {
    className?: string;
    children?: ReactNode | ReactNode[];
};

export function Card({ className, children }: CardProps) {
    return (
        <div
            className={cl(
                className,
                'rounded-2xl border border-solid border-gray-45 box-border overflow-hidden flex flex-col outline-1'
            )}
        >
            {children}
        </div>
    );
}

export { CardContent } from './content';
export { CardItem } from './item';
export { CardHeader } from './header';
export { CardFooter } from './footer';
