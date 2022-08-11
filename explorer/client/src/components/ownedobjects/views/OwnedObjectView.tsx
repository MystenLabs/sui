// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import PaginationLogic from '../../pagination/PaginationLogic';
import { type DataType, ITEMS_PER_PAGE } from '../OwnedObjectConstants';
import OwnedCoinView from './OwnedCoinView';
import OwnedNFTView from './OwnedNFTView';

import styles from '../styles/OwnedObjects.module.css';

const viewFn = (results: any) => <OwnedNFTView results={results} />;

export default function OwnedObjectView({ results }: { results: DataType }) {
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
                    <OwnedCoinView results={coin_results} />
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
