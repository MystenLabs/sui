// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cn from 'classnames';

import { Heading } from '_app/shared/heading';
import PageTitle from '_app/shared/page-title';
import { Text } from '_app/shared/text';
import Icon, { SuiIcons } from '_components/icon';

import type { ReactNode } from 'react';

import st from './CardLayout.module.scss';

export type CardLayoutProps = {
    title?: string;
    subtitle?: string;
    headerCaption?: string;
    icon?: 'success' | 'sui';
    children: ReactNode | ReactNode[];
    className?: string;
    mode?: 'box' | 'plain';
    goBackOnClick?: () => void;
};

export default function CardLayout({
    className,
    children,
    title,
    subtitle,
    headerCaption,
    icon,
    mode = 'box',
    goBackOnClick,
}: CardLayoutProps) {
    return (
        <div className={cn(className, st.container, st[mode])}>
            {goBackOnClick ? (
                <PageTitle
                    onClick={goBackOnClick}
                    hideBackLabel={true}
                    className={st.back}
                />
            ) : null}
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
            {icon === 'sui' ? (
                <div className={st.suiIconContainer}>
                    <Icon icon={SuiIcons.SuiLogoIcon} className={st.suiIcon} />
                </div>
            ) : null}
            {headerCaption ? (
                <Text variant="caption" color="steel-dark" weight="semibold">
                    {headerCaption}
                </Text>
            ) : null}
            {title ? (
                <div className="text-center mt-1.25">
                    <Heading
                        variant="heading1"
                        color="gray-90"
                        as="h1"
                        weight="bold"
                        leading="none"
                    >
                        {title}
                    </Heading>
                </div>
            ) : null}
            {subtitle ? (
                <div className="text-center mb-3.75">
                    <Text variant="caption" color="steel-darker" weight="bold">
                        {subtitle}
                    </Text>
                </div>
            ) : null}
            {children}
        </div>
    );
}
