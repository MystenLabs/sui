// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { Navigate, Route, Routes } from 'react-router-dom';

import HomePage from './pages/home';
import InitializePage from './pages/initialize';
import BackupPage from './pages/initialize/backup';
import CreatePage from './pages/initialize/create';
import ImportPage from './pages/initialize/import';
import SelectPage from './pages/initialize/select';
import WelcomePage from './pages/welcome';
import { useAppDispatch } from '_hooks';
import { loadAccountFromStorage } from '_redux/slices/account';

const App = () => {
    const dispatch = useAppDispatch();
    useEffect(() => {
        dispatch(loadAccountFromStorage());
    });
    return (
        <Routes>
            <Route path="/" element={<HomePage />} />
            <Route path="welcome" element={<WelcomePage />} />
            <Route path="/initialize" element={<InitializePage />}>
                <Route path="select" element={<SelectPage />} />
                <Route path="create" element={<CreatePage />} />
                <Route path="import" element={<ImportPage />} />
                <Route path="backup" element={<BackupPage />} />
            </Route>
            <Route path="*" element={<Navigate to="/" replace={true} />} />
        </Routes>
    );
};

export default App;
