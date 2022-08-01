// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';

import type { ReactNode, ButtonHTMLAttributes } from 'react';

import st from './Button.module.scss';

export type ButtonProps = {
    className?: string;
    mode?: 'neutral' | 'primary';
    size?: 'small' | 'large';
    children: ReactNode | ReactNode[];
    disabled?: boolean;
    onClick?: ButtonHTMLAttributes<HTMLButtonElement>['onClick'];
};

function Button({
    className,
    mode = 'neutral',
    size = 'large',
    children,
    disabled = false,
    onClick,
}: ButtonProps) {
    return (
        <button
            type="button"
            className={cl(st.container, className, st[mode], st[size])}
            onClick={onClick}
            disabled={disabled}
        >
            {children}
        </button>
    );
}

export default memo(Button);
