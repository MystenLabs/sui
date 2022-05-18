// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';

import BsIcon from '_components/bs-icon';

import type { ReactNode } from 'react';

import st from './Alert.module.scss';

export type AlertProps = {
    children: ReactNode | ReactNode[];
    className?: string;
};

function Alert({ children, className }: AlertProps) {
    return (
        <div className={cl(st.container, st.error, className)}>
            <BsIcon className={st.icon} icon="exclamation-circle" />
            <div className={st.message}>{children}</div>
        </div>
    );
}

export default memo(Alert);
