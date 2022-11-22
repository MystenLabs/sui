// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ExtractProps } from './types';

import type { ReactNode } from 'react';

import { Text } from '~/ui/Text';

export type LabelProps = ExtractProps<typeof Text>;

export function Label(props: LabelProps) {
    return (
        <dt className="col-span-1">
            <Text {...props}>{props.children}</Text>
        </dt>
    );
}

export function Value({ children }: { children: ReactNode }) {
    return <dd className="ml-0 col-span-2">{children}</dd>;
}

export type DescriptionListProps = {
    children: ReactNode[];
};

export function DescriptionList({ children }: DescriptionListProps) {
    return (
        <dl className="grid grid-cols-1 md:grid-cols-3 gap-2">{children}</dl>
    );
}
