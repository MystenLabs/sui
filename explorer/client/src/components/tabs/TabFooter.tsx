// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { numberSuffix } from '../../utils/numberUtil';

import styles from './Tabs.module.css';

//TODO: update this component to use account for multipe formats
// Update this footer now accept React.ReactElement as a child
function TabFooter({
    stats,
    children,
}: {
    children?: React.ReactElement;
    stats?: {
        count: number | string;
        stats_text: string;
    };
}) {
    return (
        <section className={styles.tabsfooter}>
            {children ? (
                [...(Array.isArray(children) ? children : [children])]
            ) : (
                <></>
            )}
            {stats && (
                <p>
                    {typeof stats.count === 'number'
                        ? numberSuffix(stats.count)
                        : stats.count}{' '}
                    {stats.stats_text}
                </p>
            )}
        </section>
    );
}

export default TabFooter;
