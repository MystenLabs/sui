// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ReactNode } from 'react';

import { Card, type CardProps } from '~/ui/Card';
import { Heading } from '~/ui/Heading';

type NetStatsProps = {
    label?: string;
    children: ReactNode;
} & CardProps;

export function NetworkStats({ label, children, ...props }: NetStatsProps) {
    return (
        <div className="inline-grid h-full w-full">
            <Card {...props}>
                <div className="grid grid-cols-1 gap-4 py-8 md:gap-8">
                    {label && (
                        <div className="px-4 md:px-8">
                            <Heading
                                color="steel-darker"
                                variant="heading4/semibold"
                            >
                                {label}
                            </Heading>
                        </div>
                    )}
                    <div className="mr-2 flex gap-8 overflow-x-auto overflow-y-hidden pl-4 md:pl-8">
                        {children}
                    </div>
                </div>
            </Card>
        </div>
    );
}
