// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';
import { type ReactNode } from 'react';

import { ReactComponent as CheckIcon } from './icons/check_12x12.svg';
import { ReactComponent as XIcon } from './icons/x.svg';

const badgeStyles = cva(
    [
        'inline-flex justify-center items-center gap-1 py-1 px-3 rounded-xl text-body font-medium',
    ],
    {
        variants: {
            variant: {
                current: 'bg-gray-40 text-steel-darker',
                success: 'bg-success-light text-success-dark',
                failure: 'bg-issue-light text-issue-dark',
            },
        },
        defaultVariants: {
            variant: 'current',
        },
    }
);

export interface BadgeProps extends VariantProps<typeof badgeStyles> {
    children?: ReactNode;
}

export function Badge({ variant, children }: BadgeProps) {
    return (
        <div className={badgeStyles({ variant })}>
            {variant === 'current' && (
                <div className="h-2 w-2 rounded-full bg-success" />
            )}
            {variant === 'failure' && <XIcon />}
            {variant === 'success' && <CheckIcon />}

            <span>{children}</span>
        </div>
    );
}
