// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import clsx from 'clsx';

import styles from './utils/customStyles.module.css';

export interface PlaceholderProps {
    width?: string;
    height?: string;
}

export function Placeholder({
    width = '100%',
    height = '1em',
}: PlaceholderProps) {
    return (
        <div
            className={clsx(
                'animate-shimmer rounded-[3px]',
                styles.placeholder
            )}
            style={{
                width,
                height,
            }}
        />
    );
}
