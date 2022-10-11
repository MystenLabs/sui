// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { memo } from 'react';

import Icon from '_components/icon';

import type { IconProps } from '_components/icon';

import st from './Item.module.scss';

export type ItemProps = {
    icon?: IconProps['icon'];
    title: string;
    subtitle?: string | null;
    indicator?: IconProps['icon'];
};

function Item({ icon, title, subtitle, indicator }: ItemProps) {
    return (
        <>
            {icon ? (
                <Icon icon={icon} className={st.icon} />
            ) : (
                <span className={st.iconPlaceholder} />
            )}
            <div className={st.title}>{title}</div>
            {subtitle ? <div className={st.subtitle}>{subtitle}</div> : null}
            {indicator ? (
                <Icon icon={indicator} className={st.indicator} />
            ) : null}
        </>
    );
}

export default memo(Item);
