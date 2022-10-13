// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import styles from './utils/customStyles.module.css';

export function Placeholder({
    width,
    height,
}: {
    width: string;
    height: string;
}) {
    return (
        <div
            className={styles.placeholder}
            style={{
                width,
                height,
            }}
        />
    );
}
