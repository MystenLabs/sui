// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { IS_STATIC_ENV } from '../../utils/envUtil';

import styles from './TxCountCard.module.css';

function TxCountCard({ count }: { count: number | string }) {
    return (
        <div className={styles.txcount} id="txcount">
            Total Transactions
            <div>{count}</div>
        </div>
    );
}

function TxCountCardStatic() {
    return <TxCountCard count={3030} />;
}

const LatestTxCard = ({ count }: { count: number }) =>
    IS_STATIC_ENV ? <TxCountCardStatic /> : <TxCountCard count={count} />;

export default LatestTxCard;
