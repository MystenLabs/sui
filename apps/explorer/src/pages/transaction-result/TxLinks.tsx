// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'clsx';
import { useState, useCallback } from 'react';

import Longtext from '../../components/longtext/Longtext';

import type { Category } from './TransactionResultType';
import type { SuiObjectRef } from '@mysten/sui.js';

import styles from './TxLinks.module.css';

import { IconTooltip } from '~/ui/Tooltip';

type Addresslist = {
    label: string;
    category?: string;
    links: SuiObjectRef[];
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
                        .map((obj, idx) => (
                            <li key={idx}>
                                <Longtext
                                    text={obj.objectId}
                                    category={data?.category as Category}
                                    isLink={true}
                                    copyButton="16"
                                    extra={
                                        <IconTooltip
                                            tip={`VERSION ${obj.version}`}
                                        />
                                    }
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
