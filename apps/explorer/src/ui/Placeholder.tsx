// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import clsx from 'clsx';

import styles from './utils/customStyles.module.css';

export interface PlaceholderProps {
    width: string;
    height: string;
}

export function Placeholder({ width, height }: PlaceholderProps) {
    return (
        <div
            className={clsx(
                'rounded-[3px] animate-shimmer',
                styles.placeholder
            )}
            style={{
                width,
                height,
            }}
        />
    );
}
