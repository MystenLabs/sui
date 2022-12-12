// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Navigate, useSearchParams } from 'react-router-dom';

import ReceiptCard from '_components/receipt-card';

function ReceiptPage() {
    const [searchParams] = useSearchParams();

    const txDigest = searchParams.get('txdigest');

    if (!txDigest) {
        return <Navigate to="/transactions" replace={true} />;
    }
    return <ReceiptCard txId={txDigest} />;
}

export default ReceiptPage;
