// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';

import { Heading } from '_app/shared/heading';
import PageTitle from '_app/shared/page-title';
import { Text } from '_app/shared/text';
import Icon, { SuiIcons } from '_components/icon';

import type { ReactNode } from 'react';

const cardLayoutStyles = cva(
    [
        'flex flex-col flex-nowrap rounded-20 items-center justify-center w-popup-width h-popup-height bg-alice-blue shadow-wallet-content',
    ],
    {
        variants: {
            mode: {
                box: 'bg-alice-blue',
                plain: 'bg-transparent',
            },
        },
        defaultVariants: {
            mode: 'box',
        },
    }
);

export interface CardLayoutProps extends VariantProps<typeof cardLayoutStyles> {
    title?: string;
    subtitle?: string;
    headerCaption?: string;
    icon?: 'success' | 'sui';
    children: ReactNode | ReactNode[];
    goBackOnClick?: () => void;
}

export default function CardLayout({
    children,
    title,
    subtitle,
    headerCaption,
    icon,
    goBackOnClick,
    ...styleProps
}: CardLayoutProps) {
    return (
        <div className={cardLayoutStyles(styleProps)}>
            <div className="p-7.5 pt-10 flex-grow flex flex-col flex-nowrap items-center justify-center w-full">
                {goBackOnClick ? (
                    <PageTitle
                        onClick={goBackOnClick}
                        hideBackLabel={true}
                        className="absolute left-[22px] top-[19px]"
                    />
                ) : null}
                {icon === 'success' ? (
                    <div className="rounded-full w-12 h-12 border-dotted border-success border-2 flex items-center justify-center mb-2.5 p-1">
                        <div className="bg-success rounded-full h-8 w-8 flex items-center justify-center">
                            <Icon
                                icon={SuiIcons.ThumbsUp}
                                className="text-white text-[25px]"
                            />
                        </div>
                    </div>
                ) : null}
                {icon === 'sui' ? (
                    <div className="flex flex-col flex-nowrap items-center justify-center rounded-full w-16 h-16 bg-sui mb-7">
                        <Icon
                            icon={SuiIcons.SuiLogoIcon}
                            className="text-white text-[34px]"
                        />
                    </div>
                ) : null}
                {headerCaption ? (
                    <Text
                        variant="caption"
                        color="steel-dark"
                        weight="semibold"
                    >
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
                        <Text
                            variant="caption"
                            color="steel-darker"
                            weight="bold"
                        >
                            {subtitle}
                        </Text>
                    </div>
                ) : null}
                {children}
            </div>
        </div>
    );
}
