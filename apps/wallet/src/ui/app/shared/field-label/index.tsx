// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '_app/shared/text';

import type { ReactNode } from 'react';

import st from './FieldLabel.module.scss';

export type FieldLabelProps = {
    txt: string;
    children: ReactNode | ReactNode[];
};

export default function FieldLabel({ txt, children }: FieldLabelProps) {
    return (
        <label className={st.container}>
            <div className="ml-2">
                <Text variant="body" color="steel-darker" weight="semibold">
                    {txt}
                </Text>
            </div>

            {children}
        </label>
    );
}
