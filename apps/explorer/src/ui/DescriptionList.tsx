// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ReactNode } from 'react';

export interface DescriptionItemProps {
    title: string | ReactNode;
    children: ReactNode;
}

export function DescriptionItem({ title, children }: DescriptionItemProps) {
    return (
        <div className="flex flex-col gap-2 md:flex-row md:items-center md:gap-10">
            <dt className="w-full flex-shrink-0 text-pBody font-medium text-steel-darker md:w-50">
                {title}
            </dt>
            <dd className="ml-0 min-w-0 flex-1 leading-none">{children}</dd>
        </div>
    );
}

export type DescriptionListProps = {
    children: ReactNode;
};

export function DescriptionList({ children }: DescriptionListProps) {
    return <dl className="mt-4 flex flex-col gap-4">{children}</dl>;
}
