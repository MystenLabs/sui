// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'clsx';
import { useState, useCallback } from 'react';

import type { SuiObjectRef } from '@mysten/sui.js';

import styles from './TxLinks.module.css';

import { ObjectLink } from '~/ui/InternalLink';
import { IconTooltip } from '~/ui/Tooltip';

type Addresslist = {
    label: string;
    category?: string;
    links: SuiObjectRef[];
};
function TxLinks({ data }: { data: Addresslist }) {
    const [viewMore, setViewMore] = useState(false);
    const numberOfListItemsToShow = 3;
    const viewAll = useCallback(() => {
        setViewMore(!viewMore);
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
                                <div className="inline-flex items-center gap-1.5">
                                    <ObjectLink
                                        objectId={obj.objectId}
                                        noTruncate
                                    />
                                    <div className="h-4 w-4 leading-none text-gray-60 hover:text-steel">
                                        <IconTooltip
                                            tip={`VERSION ${obj.version}`}
                                        />
                                    </div>
                                </div>
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
