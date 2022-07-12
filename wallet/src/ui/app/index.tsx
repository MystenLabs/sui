// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { Navigate, Route, Routes } from 'react-router-dom';

import { AppType } from './redux/slices/app/AppType';
import { useAppDispatch, useAppSelector } from '_hooks';
import { DappTxApprovalPage } from '_pages/dapp-tx-approval';
import HomePage, {
    NftsPage,
    SettingsPage,
    StakePage,
    TokensPage,
    TransactionDetailsPage,
    TransactionsPage,
    TransferCoinPage,
    TransferNFTPage,
} from '_pages/home';
import InitializePage from '_pages/initialize';
import BackupPage from '_pages/initialize/backup';
import CreatePage from '_pages/initialize/create';
import ImportPage from '_pages/initialize/import';
import SelectPage from '_pages/initialize/select';
import SiteConnectPage from '_pages/site-connect';
import WelcomePage from '_pages/welcome';
import { loadAccountFromStorage } from '_redux/slices/account';
import { loadNetworkFromStorage } from '_redux/slices/app';

const App = () => {
    const dispatch = useAppDispatch();
    useEffect(() => {
        dispatch(loadNetworkFromStorage());
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
                <Route path="stake" element={<StakePage />} />
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
            <Route path="/tx-approval/:txID" element={<DappTxApprovalPage />} />
            <Route
                path="*"
                element={<Navigate to="/tokens" replace={true} />}
            />
        </Routes>
    );
};

export default App;
