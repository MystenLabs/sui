// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { Navigate, Route, Routes } from 'react-router-dom';

import HomePage from './pages/home';
import NftsPage from './pages/home/nfts';
import TokensPage from './pages/home/tokens';
import InitializePage from './pages/initialize';
import BackupPage from './pages/initialize/backup';
import CreatePage from './pages/initialize/create';
import ImportPage from './pages/initialize/import';
import SelectPage from './pages/initialize/select';
import TransactionDetailsPage from './pages/transaction-details';
import TransferCoinPage from './pages/transfer-coin';
import WelcomePage from './pages/welcome';
import { AppType } from './redux/slices/app/AppType';
import { useAppDispatch, useAppSelector } from '_hooks';
import { loadAccountFromStorage } from '_redux/slices/account';

const App = () => {
    const dispatch = useAppDispatch();
    useEffect(() => {
        dispatch(loadAccountFromStorage());
    }, [dispatch]);
    const isPopup = useAppSelector(
        (state) => state.app.appType === AppType.popup
    );
    useEffect(() => {
        document.body.classList[isPopup ? 'add' : 'remove']('is-popup');
    }, [isPopup]);
    return (
        <Routes>
            <Route path="/" element={<HomePage />}>
                <Route
                    index
                    element={<Navigate to="/tokens" replace={true} />}
                />
                <Route path="tokens" element={<TokensPage />} />
                <Route path="nfts" element={<NftsPage />} />
                <Route path="settings" element={<h1>Settings</h1>} />
                <Route path="send" element={<TransferCoinPage />} />
                <Route
                    path="tx/:txDigest"
                    element={<TransactionDetailsPage />}
                />
            </Route>
            <Route path="welcome" element={<WelcomePage />} />
            <Route path="/initialize" element={<InitializePage />}>
                <Route path="select" element={<SelectPage />} />
                <Route path="create" element={<CreatePage />} />
                <Route path="import" element={<ImportPage />} />
                <Route path="backup" element={<BackupPage />} />
            </Route>
            <Route
                path="*"
                element={<Navigate to="/tokens" replace={true} />}
            />
        </Routes>
    );
};

export default App;
