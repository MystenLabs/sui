// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo, useMemo } from 'react';

import { SuiIcons } from '_font-icons/output/sui-icons';

import type { MouseEventHandler } from 'react';

export { SuiIcons } from '_font-icons/output/sui-icons';

export type IconProps = {
    className?: string;
    icon: SuiIcons | string;
    onClick?: MouseEventHandler<HTMLElement>;
    title?: string;
};

const isSuiIconMap: Record<string, boolean> = Object.values(SuiIcons).reduce<
    Record<string, boolean>
>((acc, anIcon) => {
    acc[anIcon] = true;
    return acc;
}, {});

function Icon({ className, icon, onClick, title }: IconProps) {
    const isSuiIcon = useMemo(() => isSuiIconMap[icon] || false, [icon]);
    return (
        <i
            className={cl(className, {
                [`bi-${icon}`]: !isSuiIcon,
                bi: !isSuiIcon,
                [icon]: isSuiIcon,
            })}
            onClick={onClick}
            title={title}
        ></i>
    );
}

export default memo(Icon);
