import { Navigate, Route, Routes } from 'react-router-dom';

import Home from '../home/Home';
import OtherDetails from '../other-details/OtherDetails';
import TransactionResult from '../transaction-result/TransactionResult';

const AppRoutes = () => {
    return (
        <Routes>
            <Route path="/" element={<Home/>} />
            <Route path="/search/:term" element={<OtherDetails />} />
            <Route path="/transactions/:id" element={<TransactionResult />} />
            <Route path="*" element={<Navigate to="/" replace={true} />} />
        </Routes>
    );
};

export default AppRoutes;
