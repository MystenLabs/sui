import { Navigate, Route, Routes } from 'react-router-dom';

import LatestTransactions from '../latesttransactions/LatestTransactions';
import OtherDetails from '../otherdetails/OtherDetails';
import TransactionResult from '../transactionresult/TransactionResult';

const AppRoutes = () => {
    return (
        <Routes>
            <Route path="/" element={<LatestTransactions />} />
            <Route path="/search/:term" element={<OtherDetails />} />
            <Route path="/transactions/:id" element={<TransactionResult />} />
            <Route path="*" element={<Navigate to="/" replace={true} />} />
        </Routes>
    );
};

export default AppRoutes;
