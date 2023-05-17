// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { type ReactNode } from 'react';

import { Heading } from '_src/ui/app/shared/heading';

interface CardProps {
    heading?: string;
    after?: ReactNode;
    children: ReactNode;
    footer?: ReactNode;
}

export const SummaryCardFooter = ({ children }: { children: ReactNode }) => {
    return (
        <div className="p-3 -m-4.5 rounded-b-2xl flex justify-between items-center bg-sui/10 ">
            {children}
        </div>
    );
};

export function Card({ heading, children, after, footer = null }: CardProps) {
    return (
        <div className="bg-white relative flex flex-col p-4.5 shadow-summary-card rounded-2xl">
            {heading && (
                <div className="flex items-center justify-between mb-4.5 last-of-type:mb-0">
                    <Heading variant="heading6" color="steel-darker">
                        {heading}
                    </Heading>
                    {after && <div>{after}</div>}
                </div>
            )}
            <div>{children}</div>
            {footer}
        </div>
    );
}
