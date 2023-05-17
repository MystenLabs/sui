// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '_app/shared/text';

import type { ReactNode } from 'react';

type IndendedTitleProps = {
    title: string;
    children: ReactNode;
};

export function IndentedTitle({ title, children }: IndendedTitleProps) {
    return (
        <div className="w-full flex flex-col justify-start gap-2">
            <div className="px-2">
                <Text variant="caption" color="steel" weight="semibold">
                    {title}
                </Text>
            </div>
            {children}
        </div>
    );
}
