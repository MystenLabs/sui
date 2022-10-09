// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'clsx';
import { useState, useCallback } from 'react';

import Longtext from '../../components/longtext/Longtext';

import type { Category } from './TransactionResultType';

import styles from './TxLinks.module.css';

type Addresslist = {
    label: string;
    category?: string;
    links: string[];
};
function TxLinks({ data }: { data: Addresslist }) {
    const [viewMore, setVeiwMore] = useState(false);
    const numberOfListItemsToShow = 3;
    const viewAll = useCallback(() => {
        setVeiwMore(!viewMore);
    }, [viewMore]);
    return (
        <div className={styles.mutatedcreatedlist}>
            <h3 className={styles.label}>{data.label}</h3>
            <div className={styles.objectidlists}>
                <ul>
                    {data.links
                        .slice(
                            0,
                            viewMore
                                ? data.links.length
                                : numberOfListItemsToShow
                        )
                        .map((objId, idx) => (
                            <li key={idx}>
                                <Longtext
                                    text={objId}
                                    category={data?.category as Category}
                                    isLink={true}
                                    copyButton="16"
                                />
                            </li>
                        ))}
                </ul>
                {data.links.length > numberOfListItemsToShow && (
                    <div className={styles.viewmore}>
                        <button
                            type="button"
                            className={cl([
                                styles.moretxbtn,
                                viewMore && styles.viewless,
                            ])}
                            onClick={viewAll}
                        >
                            {viewMore
                                ? 'View Less'
                                : 'View ' +
                                  data.links.length +
                                  ' ' +
                                  data.label}
                        </button>
                    </div>
                )}
            </div>
        </div>
    );
}

export default TxLinks;
