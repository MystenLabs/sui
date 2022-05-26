// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';

export type BsIconProps = {
    className?: string;
    icon: string;
};

function BsIcon({ className, icon }: BsIconProps) {
    return <i className={cl(className, `bi-${icon}`, 'bi')}></i>;
}

export default memo(BsIcon);
