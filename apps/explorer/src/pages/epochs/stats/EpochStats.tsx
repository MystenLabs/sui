// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ReactNode } from 'react';

import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';

interface EpochStatsProps {
    label: string;
    children: ReactNode;
}

export function EpochStats({ label, children }: EpochStatsProps) {
    return (
        <Card spacing="lg" rounded="2xl">
            <div className="flex flex-col gap-8">
                {label && (
                    <Heading color="steel-darker" variant="heading4/semibold">
                        {label}
                    </Heading>
                )}
                <div className="grid grid-cols-2 gap-8">{children}</div>
            </div>
        </Card>
    );
}
