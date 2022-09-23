// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useMemo, useEffect, useCallback, useState } from 'react';
import { Navigate, useSearchParams, useNavigate } from 'react-router-dom';

import { Content } from '_app/shared/bottom-menu-layout';
import { SuiIcons } from '_components/icon';
import Overlay from '_components/overlay';
import ReceiptCard from '_components/receipt-card';
import { useAppSelector, useAppDispatch } from '_hooks';
import { getTransactionsByAddress } from '_redux/slices/txresults';

import type { TxResultState } from '_redux/slices/txresults';

// Response pages for all transactions
// use txDigest for the transaction result
function ReceiptPage() {
    const [searchParams] = useSearchParams();
    const [showModal, setShowModal] = useState(true);
    const dispatch = useAppDispatch();
    // get tx results from url params
    const txDigest = searchParams.get('txdigest');

    const tranferType = searchParams.get('transfer') as 'nft' | 'coin';

    const txResults: TxResultState[] = useAppSelector(
        ({ txresults }) => txresults.latestTx
    );

    useEffect(() => {
        dispatch(getTransactionsByAddress()).unwrap();
    }, [dispatch]);

    const txnItem = useMemo(() => {
        return txResults.filter((txn) => txn.txId === txDigest)[0];
    }, [txResults, txDigest]);

    //TODO: redo the CTA links
    const ctaLinks = tranferType === 'nft' ? '/nfts' : '/';
    const linkTo = tranferType ? ctaLinks : '/transactions';

    const navigate = useNavigate();
    const closeReceipt = useCallback(() => {
        navigate(linkTo);
    }, [linkTo, navigate]);

    if (!txDigest && txResults && !txnItem) {
        return <Navigate to={linkTo} replace={true} />;
    }

    //TODO : add more transfer types and messages
    const transfersTxt = {
        Call: {
            sender: 'Mint Successfully',
            receiver: '',
        },
        TransferObject: {
            sender: 'Successfully Sent!',
            receiver: 'Successfully Received!',
        },
        TransferSui: {
            sender: 'Successfully Sent!',
            receiver: 'Successfully Received!',
        },
    };

    const kind = txnItem?.kind as keyof typeof transfersTxt | undefined;
    const headerCopy = kind
        ? transfersTxt[kind][txnItem.isSender ? 'sender' : 'receiver']
        : '';

    return (
        <Overlay
            showModal={showModal}
            setShowModal={setShowModal}
            title={headerCopy}
            closeOverlay={closeReceipt}
            closeIcon={SuiIcons.Checkmark}
        >
            <Content>
                {txnItem && (
                    <ReceiptCard txDigest={txnItem} tranferType={tranferType} />
                )}
            </Content>
        </Overlay>
    );
}

export default ReceiptPage;
