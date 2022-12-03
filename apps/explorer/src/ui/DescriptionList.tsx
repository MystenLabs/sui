// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ReactNode } from 'react';

import { Text } from '~/ui/Text';

export interface DescriptionItemProps {
    title: string | ReactNode;
    children: ReactNode;
}

export function DescriptionItem({ title, children }: DescriptionItemProps) {
    return (
        <div className="grid grid-cols-1 md:grid-cols-3 gap-2">
            <dt>
                {typeof title === 'string' ? (
                    <Text variant="body" weight="medium" color="steel-darker">
                        {title}
                    </Text>
                ) : (
                    title
                )}
            </dt>
            <dd className="ml-0 col-span-2 flex">{children}</dd>
        </div>
    );
}

export type DescriptionListProps = {
    children: ReactNode;
};

export function DescriptionList({ children }: DescriptionListProps) {
    return <dl className="flex flex-col gap-4">{children}</dl>;
}
