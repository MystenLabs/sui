// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import LastestTxCard from '../../components/transaction-card/LastestTxCard';

function Home() {
    return (
        <div data-testid="home-page">
            <LastestTxCard />
        </div>
    );
}

export default Home;
