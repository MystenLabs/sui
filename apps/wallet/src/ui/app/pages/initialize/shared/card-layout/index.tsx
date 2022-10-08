// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cn from 'classnames';

import Icon, { SuiIcons } from '_components/icon';

import type { ReactNode } from 'react';

import st from './CardLayout.module.scss';

export type CardLayoutProps = {
    title?: string;
    subtitle?: string;
    headerCaption?: string;
    icon?: 'success';
    children: ReactNode | ReactNode[];
    className?: string;
};

export default function CardLayout({
    className,
    children,
    title,
    subtitle,
    headerCaption,
    icon,
}: CardLayoutProps) {
    return (
        <div className={cn(className, st.container)}>
            {icon === 'success' ? (
                <div className={st.successIcon}>
                    <div className={st.successBg}>
                        <Icon
                            icon={SuiIcons.ThumbsUp}
                            className={st.thumbsUp}
                        />
                    </div>
                </div>
            ) : null}
            {headerCaption ? (
                <h3 className={st.caption}>{headerCaption}</h3>
            ) : null}
            {title ? <h1 className={st.headerTitle}>{title}</h1> : null}
            {subtitle ? <h1 className={st.subTitle}>{subtitle}</h1> : null}
            {children}
        </div>
    );
}
