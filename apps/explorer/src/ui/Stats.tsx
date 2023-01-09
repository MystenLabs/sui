// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ReactComponent as InfoSvg } from './icons/info_10x10.svg';

import type { ReactNode } from 'react';

import { Text } from '~/ui/Text';
import { Tooltip } from '~/ui/Tooltip';

export type StatsProps = {
    label: string;
    children: ReactNode;
    tooltip?: string;
};

export function Stats({ label, children, tooltip }: StatsProps) {
    return (
        <div className="flex max-w-full flex-col flex-nowrap gap-1.5">
            <div className="flex items-center justify-start gap-1 text-caption text-steel-dark hover:text-steel">
                <Text variant="caption/semibold" color="steel-dark">
                    {label}
                </Text>
                {tooltip && (
                    <Tooltip tip={tooltip}>
                        <InfoSvg />
                    </Tooltip>
                )}
            </div>
            {children}
        </div>
    );
}
