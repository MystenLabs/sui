// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { memo } from 'react';

import type { ReactNode } from 'react';

import st from './Field.module.scss';

export type FieldProps = {
    name: string;
    children: ReactNode;
};

function Field({ name, children }: FieldProps) {
    return (
        <div className={st.field}>
            <span className={st.name}>{name}</span>
            <span className={st.value}>{children}</span>
        </div>
    );
}

export default memo(Field);
