// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cn from 'classnames';

import type { FieldProps } from 'formik';
import type { ReactNode } from 'react';

import st from './InputWithAction.module.scss';

export type InputWithActionProps = FieldProps & {
    children: ReactNode | ReactNode[];
    className?: string;
};

export default function InputWithAction({
    field,
    meta,
    form,
    children,
    className,
    ...props
}: InputWithActionProps) {
    return (
        <>
            <div className={st.container}>
                <input
                    type="number"
                    {...field}
                    {...props}
                    className={cn(st.input, className)}
                />
                <div className={st.actionContainer}>{children}</div>
            </div>
        </>
    );
}
