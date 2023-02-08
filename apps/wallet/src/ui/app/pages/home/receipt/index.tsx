// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getExecutionStatusType, getTransactionKindName } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';
import { useCallback, useMemo, useState } from 'react';
import { Navigate, useSearchParams, useNavigate } from 'react-router-dom';

import Alert from '_components/alert';
import { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import Overlay from '_components/overlay';
import ReceiptCard from '_components/receipt-card';
import { checkStakingTxn } from '_helpers';
import { useRpc, useAppSelector } from '_hooks';

function ReceiptPage() {
    const [searchParams] = useSearchParams();
    const [showModal, setShowModal] = useState(true);
    const activeAddress = useAppSelector(({ account: { address } }) => address);

    // get tx results from url params
    const transactionId = searchParams.get('txdigest');
    // get Return route from URL params
    const fromRoute = searchParams.get('from');
    const rpc = useRpc();

    const { data, isLoading, isError } = useQuery(
        ['transactions-by-id', transactionId],
        async () => {
            return rpc.getTransactionWithEffects(transactionId!);
        },
        { enabled: !!transactionId, retry: 8 }
    );

    // return route or default to transactions
    const linkTo = fromRoute ? `/${fromRoute}` : '/transactions';

    const navigate = useNavigate();
    const closeReceipt = useCallback(() => {
        navigate(linkTo);
    }, [linkTo, navigate]);

    const pageTitle = useMemo(() => {
        if (data) {
            const executionStatus = getExecutionStatusType(data);
            const { sender, transactions } = data.certificate.data;

            const txnKind = getTransactionKindName(transactions[0]);
            const stakingTxn = checkStakingTxn(data);

            const isTransfer =
                txnKind === 'PaySui' ||
                txnKind === 'TransferSui' ||
                txnKind === 'PayAllSui' ||
                txnKind === 'TransferObject' ||
                txnKind === 'Pay';

            const isSender = activeAddress === sender;

            const transferName = isTransfer
                ? isSender
                    ? 'Sent Successfully'
                    : 'Received Successfully'
                : stakingTxn
                ? stakingTxn + ' Successfully'
                : 'Move Call';

            return `${
                executionStatus === 'success'
                    ? transferName
                    : 'Transaction Failed'
            }`;
        }

        return 'Transaction Failed';
    }, [activeAddress, data]);

    if (!transactionId || !activeAddress) {
        return <Navigate to={linkTo} replace={true} />;
    }

    return (
        <Loading
            loading={isLoading}
            className="flex items-center justify-center"
        >
            <Overlay
                showModal={showModal}
                setShowModal={setShowModal}
                title={pageTitle}
                closeOverlay={closeReceipt}
                closeIcon={SuiIcons.Check}
            >
                {isError ? (
                    <Alert className="mb-2 h-fit">Something went wrong</Alert>
                ) : null}

                {data && (
                    <ReceiptCard txn={data} activeAddress={activeAddress} />
                )}
            </Overlay>
        </Loading>
    );
}

export default ReceiptPage;
