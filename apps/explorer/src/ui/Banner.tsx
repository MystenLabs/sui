// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Info16 as InfoIcon } from '@mysten/icons';
import { cva, type VariantProps } from 'class-variance-authority';
import { type ReactNode } from 'react';

import { IconButton } from './IconButton';

const bannerStyles = cva(
    'inline-flex text-p2 font-medium rounded-lg overflow-hidden box-border gap-2 items-center flex-nowrap relative',
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
            spacing: {
                md: 'px-3 py-2',
                lg: 'p-5',
            },
        },
        defaultVariants: {
            variant: 'message',
            spacing: 'md',
        },
    }
);

export interface BannerProps extends VariantProps<typeof bannerStyles> {
    icon?: ReactNode | null;
    children: ReactNode;
    onDismiss?: () => void;
}

export function Banner({
    icon = <InfoIcon />,
    children,
    variant,
    align,
    fullWidth,
    spacing,
    onDismiss,
}: BannerProps) {
    return (
        <div
            className={bannerStyles({
                variant,
                align,
                fullWidth,
                spacing,
                class: onDismiss && 'pr-9',
            })}
        >
            {icon && (
                <div className="flex items-center justify-center">{icon}</div>
            )}
            <div className="overflow-hidden break-words">{children}</div>
            {onDismiss ? (
                <div className="absolute top-0 right-0">
                    <IconButton
                        icon="x"
                        onClick={onDismiss}
                        aria-label="Close"
                    />
                </div>
            ) : null}
        </div>
    );
}
