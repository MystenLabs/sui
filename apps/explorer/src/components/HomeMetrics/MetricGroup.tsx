// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ReactNode } from 'react';

import { Text } from '~/ui/Text';

interface Props {
    label: string;
    children: ReactNode;
}

export function MetricGroup({ label, children }: Props) {
    return (
        <div className="flex flex-col gap-4">
            <div className="flex items-center gap-2.5">
                <Text variant="caption/semibold" color="steel-darker">
                    {label}
                </Text>
                <div className="h-px flex-1 bg-steel/30" />
            </div>
            <div className="flex items-start gap-10 overflow-x-auto overflow-y-visible">
                {children}
            </div>
        </div>
    );
}
