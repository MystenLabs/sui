// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Info16, CheckStroke16 } from '@mysten/icons';
import { cva, type VariantProps } from 'class-variance-authority';

import LoadingIndicator from '_components/loading/LoadingIndicator';

import type { ReactNode } from 'react';

const alertStyles = cva(
    'rounded-2xl text-pBodySmall font-medium flex flex-row flex-nowrap justify-start items-center py-2 px-2.5 gap-2',
    {
        variants: {
            mode: {
                warning:
                    'border-solid border bg-issue-light border-issue-dark/20 text-issue-dark',
                success:
                    'border-solid border bg-success-light border-success-dark/20 text-success-dark',
                loading: 'bg-steel text-white border-warning-dark/20',
            },
        },
        defaultVariants: {
            mode: 'warning',
        },
    }
);

export interface AlertProps extends VariantProps<typeof alertStyles> {
    children: ReactNode;
    mode?: 'warning' | 'loading' | 'success';
}

const modeToIcon = {
    warning: <Info16 className="h-3.5 w-3.5" />,
    success: <CheckStroke16 className="h-3 w-3" />,
    loading: <LoadingIndicator color="inherit" />,
};

export default function Alert({ children, mode = 'warning' }: AlertProps) {
    return (
        <div className={alertStyles({ mode })}>
            {modeToIcon[mode]}
            <div className="break-all flex-1">{children}</div>
        </div>
    );
}
