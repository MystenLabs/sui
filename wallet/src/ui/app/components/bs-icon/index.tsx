// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';

import type { MouseEventHandler } from 'react';

export type BsIconProps = {
    className?: string;
    icon: string;
    onClick?: MouseEventHandler<HTMLElement>;
    title?: string;
};

function BsIcon({ className, icon, onClick, title }: BsIconProps) {
    return (
        <i
            className={cl(className, `bi-${icon}`, 'bi')}
            onClick={onClick}
            title={title}
        ></i>
    );
}

export default memo(BsIcon);
