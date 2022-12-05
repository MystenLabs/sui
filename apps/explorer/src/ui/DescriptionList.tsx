// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ReactNode } from 'react';

export interface DescriptionItemProps {
    title: string | ReactNode;
    children: ReactNode;
}

export function DescriptionItem({ title, children }: DescriptionItemProps) {
    return (
        <div className="flex flex-col md:flex-row items-start gap-2 md:gap-10">
            <dt className="w-full md:w-50 text-steel-darker font-medium text-p1">
                {title}
            </dt>
            <dd className="ml-0 flex-1 leading-none">{children}</dd>
        </div>
    );
}

export type DescriptionListProps = {
    children: ReactNode;
};

export function DescriptionList({ children }: DescriptionListProps) {
    return <dl className="flex flex-col gap-4">{children}</dl>;
}
