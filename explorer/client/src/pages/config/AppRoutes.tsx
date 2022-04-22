// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Navigate, Route, Routes } from 'react-router-dom';

import AddressResult from '../address-result/AddressResult';
import Home from '../home/Home';
import MissingResource from '../missing-resource/MissingResource';
import { ObjectResult } from '../object-result/ObjectResult';
import TransactionResult from '../transaction-result/TransactionResult';

const AppRoutes = () => {
    return (
        <Routes>
            <Route path="/" element={<Home />} />
            <Route path="/objects/:id" element={<ObjectResult />} />
            <Route path="/transactions/:digest" element={<TransactionResult />} />
            <Route path="/addresses/:id" element={<AddressResult />} />
            <Route path="/missing/:id" element={<MissingResource />} />
            <Route path="*" element={<Navigate to="/" replace={true} />} />
        </Routes>
    );
};

export default AppRoutes;
