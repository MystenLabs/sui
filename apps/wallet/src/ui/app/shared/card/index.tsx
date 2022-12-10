// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';

import type { ReactNode } from 'react';

export type CardProps = {
    className?: string;
    children?: ReactNode | ReactNode[];
};

function Card({ className, children }: CardProps) {
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

export default memo(Card);
export { default as CardContent } from './content';
export { default as CardItem } from './item';
export { default as CardHeader } from './header';
export { default as CardFooter } from './footer';
