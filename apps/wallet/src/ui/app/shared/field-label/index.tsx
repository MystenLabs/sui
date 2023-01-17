// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '_app/shared/text';

import type { ReactNode } from 'react';

export type FieldLabelProps = {
    txt: string;
    children: ReactNode | ReactNode[];
};

export default function FieldLabel({ txt, children }: FieldLabelProps) {
    return (
        <label className="flex flex-col flex-nowrap first:-mt-7.5">
            <div className="ml-2 mt-7.5 mb-2.5">
                <Text variant="body" color="steel-darker" weight="semibold">
                    {txt}
                </Text>
            </div>

            {children}
        </label>
    );
}
