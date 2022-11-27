// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva } from 'class-variance-authority';

import { ReactComponent as SpinnerSvg } from './icons/spinner.svg';

import type { VariantProps } from 'class-variance-authority';

const spinnerStyles = cva(null, {
    variants: {
        size: {
            md: '',
        },
        default: {
            size: 'md',
        },
    },
});

export interface LoadingSpinnerProps
    extends VariantProps<typeof spinnerStyles> {
    text?: string;
}

export function LoadingSpinner({ text }: LoadingSpinnerProps) {
    return (
        <div className="inline-flex flex-row flex-nowrap items-center gap-3 text-body font-medium">
            <SpinnerSvg className="animate-spin text-steel" />
            {text ? <div className="text-steel-dark">{text}</div> : null}
        </div>
    );
}
