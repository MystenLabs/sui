// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import Longtext from '../../components/longtext/Longtext';
import { numberSuffix } from '../../utils/numberUtil';

import styles from './Tabs.module.css';

function TabFooter({
    link,
    stats,
}: {
    link: {
        text: string;
        categoryName:
            | 'objects'
            | 'transactions'
            | 'addresses'
            | 'ethAddress'
            | 'unknown';
        isCopyButton?: boolean;
        alttext?: string;
    };
    stats?: {
        count: number | string;
        stats_text: string;
    };
}) {
    return (
        <section className={styles.tabsfooter}>
            <Longtext
                text={link.text}
                category={link.categoryName}
                isLink={true}
                isCopyButton={link.isCopyButton}
                showIconButton={true}
                alttext={link.alttext}
            />
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
