// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Longtext from '../../components/longtext/Longtext';
import ViewMore from '../../components/view-more/ViewMore';

import type { Category } from './TransactionResultType';

import styles from './TxLinks.module.css';

type Addresslist = {
    label: string;
    category?: string;
    links: string[];
};
const NUMBER_OF_ITEMS_TO_SHOW = 3;

function TxLinks({ data }: { data: Addresslist }) {
    return (
        <div className={styles.mutatedcreatedlist}>
            <h3 className={styles.label}>{data.label}</h3>
            <div className={styles.objectidlists}>
                <ul>
                    <ViewMore
                        label={data.label}
                        limitTo={NUMBER_OF_ITEMS_TO_SHOW}
                    >
                        {data.links.map((objId, idx) => (
                            <li key={idx}>
                                <Longtext
                                    text={objId}
                                    category={data?.category as Category}
                                    isLink={true}
                                />
                            </li>
                        ))}
                    </ViewMore>
                </ul>
            </div>
        </div>
    );
}

export default TxLinks;
