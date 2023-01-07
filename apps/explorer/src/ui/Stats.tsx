// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ReactComponent as InfoSvg } from './icons/info_10x10.svg';

import { Heading } from '~/ui/Heading';
import { Text } from '~/ui/Text';
import { Tooltip } from '~/ui/Tooltip';

export type StatsProps = {
    label: string;
    value: string | number;
    tooltip?: string;
};
export function Stats({ label, value, tooltip }: StatsProps) {
    return (
        <div className="flex max-w-full flex-col flex-nowrap gap-1.5">
            <div className="flex items-center justify-start gap-0.5 text-caption text-steel-dark hover:text-steel">
                <Text variant="caption/semibold" color="steel-dark">
                    {label}
                </Text>
                {tooltip && (
                    <Tooltip tip={tooltip}>
                        <InfoSvg />
                    </Tooltip>
                )}
            </div>

            <Heading as="h3" variant="heading2/semibold" color="steel-darker">
                {value}
            </Heading>
        </div>
    );
}
