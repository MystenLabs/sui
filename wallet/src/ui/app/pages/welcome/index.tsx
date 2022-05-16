// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Link } from 'react-router-dom';

import Loading from '_components/loading';
import Logo from '_components/logo';
import { useFullscreenGuard, useInitializedGuard } from '_hooks';

import st from './Welcome.module.scss';

const WelcomePage = () => {
    const checkingInitialized = useInitializedGuard(false);
    const checkingFullscreen = useFullscreenGuard();
    const guardChecking = checkingFullscreen || checkingInitialized;
    return (
        <Loading loading={guardChecking}>
            <div className={st.container}>
                <Logo size="bigger" />
                <h1 className={st.title}>Welcome to Sui Wallet</h1>
                <Link to="/initialize/select" className="btn">
                    Get Started
                </Link>
            </div>
        </Loading>
    );
};

export default WelcomePage;
