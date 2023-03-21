// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ReactComponent as InfoSvg } from './icons/info_10x10.svg';

import type { ReactNode } from 'react';

import { Heading } from '~/ui/Heading';
import { Text } from '~/ui/Text';
import { Tooltip } from '~/ui/Tooltip';

export type StatsProps = {
    size?: 'sm' | 'md';
    label: string;
    children?: ReactNode;
    tooltip?: string;
    unavailable?: boolean;
};

export function Stats({
    label,
    children,
    tooltip,
    unavailable,
    size = 'md',
}: StatsProps) {
    return (
        <div className="flex max-w-full flex-col flex-nowrap gap-1.5">
            <div className="flex items-center justify-start gap-1 text-caption text-steel-dark hover:text-steel">
                <div className="flex-shrink-0">
                    <Text variant="caption/semibold" color="steel-dark">
                        {label}
                    </Text>
                </div>
                {tooltip && (
                    <Tooltip tip={unavailable ? 'Coming soon' : tooltip}>
                        <InfoSvg />
                    </Tooltip>
                )}
            </div>
            <Heading
                variant={
                    size === 'md' ? 'heading2/semibold' : 'heading3/semibold'
                }
                color={unavailable ? 'steel-dark' : 'steel-darker'}
            >
                {unavailable || !children ? '--' : children}
            </Heading>
        </div>
    );
}
