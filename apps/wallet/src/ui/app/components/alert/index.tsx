// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Info16, Check32 } from '@mysten/icons';
import cl from 'classnames';

import LoadingIndicator from '_components/loading/LoadingIndicator';

import type { ReactNode } from 'react';

import st from './Alert.module.scss';

type ModeType = 'warning' | 'loading' | 'success';
export type AlertProps = {
    children: ReactNode | ReactNode[];
    className?: string;
    mode?: ModeType;
};
const modeToIcon = {
    warning: <Info16 />,
    success: <Check32 />,
    loading: <LoadingIndicator color="inherit" />,
};

export default function Alert({
    children,
    className,
    mode = 'warning',
}: AlertProps) {
    return (
        <div className={cl(st.container, st[mode], className)}>
            {modeToIcon[mode]}
            <div className={st.message}>{children}</div>
        </div>
    );
}
