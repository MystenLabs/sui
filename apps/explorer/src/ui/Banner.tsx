// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';
import { type ReactNode } from 'react';

import { Button } from './Button';
import { ReactComponent as InfoIcon } from './icons/info.svg';
import { ReactComponent as CloseIcon } from './icons/x.svg';

const bannerStyles = cva(
    'inline-flex text-p2 font-medium rounded-lg overflow-hidden box-border gap-1',
    {
        variants: {
            variant: {
                positive: 'bg-success-light text-success-dark',
                warning: 'bg-warning-light text-warning-dark',
                error: 'bg-issue-light text-issue-dark',
                message: 'bg-sui-light text-hero',
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

const contentStyles = cva(
    'flex flex-1 items-center gap-2 max-w-full min-w-0 flex-nowrap',
    {
        variants: {
            align: {
                left: 'justify-start',
                center: 'justify-center',
            },
        },
        defaultVariants: {
            align: 'left',
        },
    }
);

const closeBtnStyles = cva('self-start', {
    variants: {
        spacing: {
            md: '-mt-1 -mr-2',
            lg: '-mt-4 -mr-4',
        },
    },
    defaultVariants: {
        spacing: 'md',
    },
});

export interface BannerProps
    extends VariantProps<typeof bannerStyles>,
        VariantProps<typeof contentStyles> {
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
        <div className={bannerStyles({ variant, fullWidth, spacing })}>
            <div
                className={contentStyles({
                    align,
                })}
            >
                {icon && (
                    <div className="flex items-center justify-center">
                        {icon}
                    </div>
                )}
                <div className="overflow-hidden break-words">{children}</div>
            </div>
            {onDismiss ? (
                <div className={closeBtnStyles({ spacing })}>
                    <Button onClick={onDismiss} variant="txt" size="md">
                        <CloseIcon />
                    </Button>
                </div>
            ) : null}
        </div>
    );
}
