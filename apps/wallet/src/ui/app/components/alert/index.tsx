// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';

import Icon, { SuiIcons } from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';

import type { ReactNode } from 'react';

import st from './Alert.module.scss';

type ModeType = 'warning' | 'loading' | 'success';
export type AlertProps = {
    children: ReactNode | ReactNode[];
    className?: string;
    mode?: ModeType;
};
const modeToIcon: Record<Exclude<ModeType, 'loading'>, SuiIcons> = {
    warning: SuiIcons.Info,
    success: SuiIcons.Check,
};

export default function Alert({
    children,
    className,
    mode = 'warning',
}: AlertProps) {
    return (
        <div className={cl(st.container, st[mode], className)}>
            {mode === 'loading' ? (
                <LoadingIndicator color="inherit" />
            ) : (
                <Icon className={st.icon} icon={modeToIcon[mode]} />
            )}
            <div className={st.message}>{children}</div>
        </div>
    );
}
