// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { Navigate, Route, Routes } from 'react-router-dom';

import HomePage from './pages/home';
import NftsPage from './pages/home/nfts';
import SettingsPage from './pages/home/settings';
import TokensPage from './pages/home/tokens';
import TransactionsPage from './pages/home/transactions';
import InitializePage from './pages/initialize';
import BackupPage from './pages/initialize/backup';
import CreatePage from './pages/initialize/create';
import ImportPage from './pages/initialize/import';
import SelectPage from './pages/initialize/select';
import SiteConnectPage from './pages/site-connect';
import TransactionDetailsPage from './pages/transaction-details';
import TransferCoinPage from './pages/transfer-coin';
import TransferNFTPage from './pages/transfer-nft';
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
                <Route path="transactions" element={<TransactionsPage />} />
                <Route path="settings" element={<SettingsPage />} />
                <Route path="send" element={<TransferCoinPage />} />
                <Route path="send-nft" element={<TransferNFTPage />} />
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
            <Route path="/connect/:requestID" element={<SiteConnectPage />} />
            <Route
                path="*"
                element={<Navigate to="/tokens" replace={true} />}
            />
        </Routes>
    );
};

export default App;
