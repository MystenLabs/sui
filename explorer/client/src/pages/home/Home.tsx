// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';

// import SuiNetworkStats from '../../components/network-stats/SuiNetworkStats';
// import TopGroupsCard from '../../components/top-groups/TopGroups';
// import TopValidatorsCard from '../../components/top-validators-card/TopValidatorsCard';

import LastestTxCard from '../../components/transaction-card/RecentTxCard';

import styles from './Home.module.css';

const Home = () => (
    <div
        data-testid="home-page"
        id="home"
        className={cl([styles.home, styles.container])}
    >
        <section className="left-item">
            <LastestTxCard />
        </section>
        <section className="right-item"></section>
    </div>
);

export default Home;
