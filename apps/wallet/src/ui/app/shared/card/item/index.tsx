// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';

import { Text } from '_app/shared/text';

import type { ReactNode } from 'react';




const cardContentStyle = cva(
    ['divide-x flex divide-solid divide-gray-45 divide-y-0'],
    {
        variants: {
            padding: {
                true: 'p-4',
            },
            col: {
                true: 'flex-col',
            },
            gap: {
                true: 'gap-3.5',
            },
            colored: {
                true: 'bg-sui/10',
            },
        },
        defaultVariants: {
            padding: false,
            col: false,
            gap: false,
            colored: false,
        },
    }
);


export type CardItemProps = {
    title: ReactNode;
    value: ReactNode;
};

export function CardItem({ title, value }: CardItemProps) {
    return (
        <div
            className={'flex flex-col flex-nowrap max-w-full p-3.5 gap-1.5 flex-1 justify-center items-center'}
        >
            <Text variant="captionSmall" weight="semibold" color="steel-darker">
                {title}
            </Text>

            <div className="overflow-x-hidden text-ellipsis">{value}</div>
        </div>
    );
}