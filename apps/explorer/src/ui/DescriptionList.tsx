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
        <div className="flex flex-col md:flex-row items-start gap-2 md:gap-10">
            <dt className="w-full md:w-48">
                {typeof title === 'string' ? (
                    <Text variant="body" weight="medium" color="steel-darker">
                        {title}
                    </Text>
                ) : (
                    title
                )}
            </dt>
            <dd className="ml-0 flex flex-1">{children}</dd>
        </div>
    );
}

export type DescriptionListProps = {
    children: ReactNode;
};

export function DescriptionList({ children }: DescriptionListProps) {
    return <dl className="flex flex-col gap-4">{children}</dl>;
}
