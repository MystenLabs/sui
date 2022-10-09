// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ReactNode } from 'react';

import st from './FieldLabel.module.scss';

export type FieldLabelProps = {
    txt: string;
    children: ReactNode | ReactNode[];
};

export default function FieldLabel({ txt, children }: FieldLabelProps) {
    return (
        <label className={st.container}>
            <span className={st.label}>{txt}</span>
            {children}
        </label>
    );
}
