import { Navigate, Route, Routes } from 'react-router-dom';

import Home from '../Home';
import Search from '../Search';
import TransactionDetails from '../TransactionDetails';

const AppRoutes = () => {
    return (
        <Routes>
            <Route path="/" element={<Home />} />
            <Route path="/search/:term" element={<Search />} />
            <Route path="/transactions/:id" element={<TransactionDetails />} />
            <Route path="*" element={<Navigate to="/" replace={true} />} />
        </Routes>
    );
};

export default AppRoutes;
