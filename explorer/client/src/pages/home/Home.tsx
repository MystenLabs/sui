// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import LastestTxCard from '../../components/transaction-card/RecentTxCard';
import TxCountCard from '../../components/transaction-count/TxCountCard';

import styles from './Home.module.css';

function Home() {
    return (
        <div data-testid="home-page" id="home" className={styles.home}>
            <LastestTxCard />
            <TxCountCard />
        </div>
    );
}

export default Home;
