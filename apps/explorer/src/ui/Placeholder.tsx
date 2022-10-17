// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva } from 'class-variance-authority';

import styles from './utils/customStyles.module.css';

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

export function Placeholder({
    width,
    height,
}: {
    width: string;
    height: string;
}) {
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
