// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';

import type { ReactNode } from 'react';

const DescriptionItemStyles = cva(
    ['flex flex-col gap-2 md:flex-row md:gap-10'],
    {
        variants: {
            align: {
                start: 'md:items-start',
                center: 'md:items-center',
            },
        },
        defaultVariants: {
            align: 'center',
        },
    }
);

type DescriptionItemStylesProps = VariantProps<typeof DescriptionItemStyles>;

export interface DescriptionItemProps extends DescriptionItemStylesProps {
    title: string | ReactNode;
    children: ReactNode;
}

export function DescriptionItem({
    title,
    align,
    children,
}: DescriptionItemProps) {
    return (
        <div className={DescriptionItemStyles({ align })}>
            <dt className="w-full flex-shrink-0 text-pBody font-medium text-steel-darker md:w-40">
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
