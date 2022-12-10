// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';

import type { ReactNode } from 'react';

export interface CardFooterProps {
    className?: string;
    children: ReactNode;
}

function CardFooter({ children, className }: CardFooterProps) {
    return (
        <div
            className={cl(
                className,
                'flex flex-col pt-0 justify-center w-full p-3.5 '
            )}
        >
            <span className="h-px w-full bg-gray-45 lg:w-1/3 px-4"></span>
            <div className="flex justify-between pt-3.5">{children}</div>
        </div>
    );
}

export default memo(CardFooter);
