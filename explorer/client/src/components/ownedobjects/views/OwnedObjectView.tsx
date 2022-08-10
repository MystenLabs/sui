// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {type DataType} from '../OwnedObjectResultType';
import styles from './OwnedObject.module.css';

export function OwnedObjectLayout({ results }: { results: DataType}) {
    const coin_results = results.filter(({ _isCoin }) => _isCoin);
    const other_results = results
        .filter(({ _isCoin }) => !_isCoin)
        .sort((a, b) => {
            if (a.Type > b.Type) return 1;
            if (a.Type < b.Type) return -1;
            if (a.Type === b.Type) {
                return a.id <= b.id ? -1 : 1;
            }
            return 0;
        });

    const nftFooter = {
        stats: {
            count: other_results.length,
            stats_text: 'Total NFTs',
        },
    };

    return (
        <div className={styles.layout}>
            {coin_results.length > 0 && (
                <div>
                    <div className={styles.ownedobjectheader}>
                        <h2>Coins</h2>
                    </div>
                    <GroupView results={coin_results} />
                </div>
            )}
            {other_results.length > 0 && (
                <div id="NFTSection">
                    <div className={styles.ownedobjectheader}>
                        <h2>NFTs</h2>
                    </div>
                    <PaginationLogic
                        results={other_results}
                        viewComponentFn={viewFn}
                        itemsPerPage={ITEMS_PER_PAGE}
                        stats={nftFooter.stats}
                    />
                </div>
            )}
        </div>
    );
}
