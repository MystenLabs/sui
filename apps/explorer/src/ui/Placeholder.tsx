// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva } from 'class-variance-authority';

import styles from './utils/customStyles.module.css';

export interface PlaceholderProps {
    width: string;
    height: string;
}

const placeholder = cva([styles.placeholder], {
    variants: {
        variant: {
            default: 'rounded-[3px] animate-shimmer',
        },
    },
    defaultVariants: {
        variant: 'default',
    },
});

export function Placeholder({ width, height }: PlaceholderProps) {
    return (
        <div
            className={placeholder()}
            style={{
                width,
                height,
            }}
        />
    );
}
