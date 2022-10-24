// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';
import { type ReactNode } from 'react';

import { ReactComponent as InfoIcon } from './icons/info.svg';

const bannerStyles = cva(
    'inline-flex items-center gap-2 text-p2 font-medium rounded-lg px-3 py-2',
    {
        variants: {
            variant: {
                positive: 'bg-success-light text-success-dark',
                warning: 'bg-warning-light text-warning-dark',
                error: 'bg-issue-light text-issue-dark',
                message: 'bg-sui-light text-hero',
            },
            align: {
                left: 'justify-start',
                center: 'justify-center',
            },
            fullWidth: {
                true: 'w-full',
            },
        },
        defaultVariants: {
            variant: 'message',
        },
    }
);

export interface BannerProps extends VariantProps<typeof bannerStyles> {
    icon?: ReactNode | null;
    children: ReactNode;
}

export function Banner({
    icon = <InfoIcon />,
    children,
    variant,
    align,
    fullWidth,
}: BannerProps) {
    return (
        <div className={bannerStyles({ variant, align, fullWidth })}>
            {icon && (
                <div className="flex items-center justify-center">{icon}</div>
            )}
            {children}
        </div>
    );
}
