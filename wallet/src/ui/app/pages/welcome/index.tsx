// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Link } from 'react-router-dom';

import Loading from '_components/loading';
import Logo from '_components/logo';
import { useInitializedGuard } from '_hooks';
import PageLayout from '_pages/layout';

import st from './Welcome.module.scss';

const WelcomePage = () => {
    const checkingInitialized = useInitializedGuard(false);
    return (
        <PageLayout forceFullscreen={true}>
            <Loading loading={checkingInitialized}>
                <div className={st.container}>
                    <Logo size="bigger" />
                    <h1 className={st.title}>Welcome to Sui Wallet</h1>
                    <Link to="/initialize/select" className="btn">
                        Get Started
                    </Link>
                </div>
            </Loading>
        </PageLayout>
    );
};

export default WelcomePage;
