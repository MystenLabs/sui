// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';
import { Link } from 'react-router-dom';

import { type TxResultState } from '../../hooks/useRecentTransactions';
import Icon, { SuiIcons } from '_components/icon';
import { formatDate } from '_helpers';
import { useMiddleEllipsis, useFormatCoin } from '_hooks';
import { GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';

import st from './TransactionsCard.module.scss';

const TRUNCATE_MAX_LENGTH = 8;
const TRUNCATE_PREFIX_LENGTH = 4;

// Truncate text after one line (~ 35 characters)
const TRUNCATE_MAX_CHAR = 35;

function TransactionCard({ txn }: { txn: TxResultState }) {
    const toAddrStr = useMiddleEllipsis(
        txn.to || '',
        TRUNCATE_MAX_LENGTH,
        TRUNCATE_PREFIX_LENGTH
    );
    const fromAddrStr = useMiddleEllipsis(
        txn.from || '',
        TRUNCATE_MAX_LENGTH,
        TRUNCATE_PREFIX_LENGTH
    );

    const truncatedNftName = useMiddleEllipsis(
        txn?.name || '',
        TRUNCATE_MAX_CHAR,
        TRUNCATE_MAX_CHAR - 1
    );
    const truncatedNftDescription = useMiddleEllipsis(
        txn?.description || '',
        TRUNCATE_MAX_CHAR,
        TRUNCATE_MAX_CHAR - 1
    );

    // TODO: update to account for bought, minted, swapped, etc
    const transferType =
        txn.kind === 'Call' ? 'Call' : txn.isSender ? 'Sent' : 'Received';

    const amount = txn?.amount || txn?.balance || txn?.txGas || 0;

    const transferMeta = {
        Call: {
            // For NFT with name and image use Mint else use Call (Function Name)
            txName: txn.name && txn.url ? 'Minted' : 'Call',
            transfer: false,
            address: false,
            icon: SuiIcons.Buy,
            iconClassName: cl(st.arrowActionIcon, st.buyIcon),
            amount: amount,
        },
        Sent: {
            txName: 'Sent',
            transfer: 'To',
            address: toAddrStr,
            icon: SuiIcons.ArrowLeft,
            iconClassName: cl(st.arrowActionIcon, st.angledArrow),
            amount: amount,
        },
        Received: {
            txName: 'Received',
            transfer: 'From',
            address: fromAddrStr,
            icon: SuiIcons.ArrowLeft,
            iconClassName: cl(st.arrowActionIcon, st.angledArrow, st.received),
            amount: amount,
        },
    };

    const date = txn?.timestampMs
        ? formatDate(txn.timestampMs, ['month', 'day', 'hour', 'minute'])
        : false;

    const transferSuiTxn = txn.kind === 'TransferSui' ? <span>SUI</span> : null;
    const transferFailed = txn.error ? (
        <div className={st.transferFailed}>{txn.error}</div>
    ) : null;

    const txnsAddress = transferMeta[transferType]?.address ? (
        <div className={st.address}>
            <div className={st.txTypeName}>
                {transferMeta[transferType].transfer}
            </div>
            <div className={cl(st.txValue, st.txAddress)}>
                {transferMeta[transferType].address}
            </div>
        </div>
    ) : null;

    const callFnName = txn?.callFunctionName ? (
        <span className={st.callFnName}>({txn?.callFunctionName})</span>
    ) : null;

    const [formattedAmount, symbol] = useFormatCoin(
        transferMeta[transferType].amount,
        txn.coinType || GAS_TYPE_ARG
    );

    return (
        <Link
            to={`/receipt?${new URLSearchParams({
                txdigest: txn.txId,
            }).toString()}`}
            className={st.txCard}
        >
            <div className={st.card} key={txn.txId}>
                <div className={st.cardIcon}>
                    <Icon
                        icon={transferMeta[transferType].icon}
                        className={transferMeta[transferType].iconClassName}
                    />
                </div>
                <div className={st.cardContent}>
                    <div className={st.txResult}>
                        <div className={cl(st.txTypeName, st.kind)}>
                            {txn.error
                                ? 'Transaction failed'
                                : transferMeta[transferType].txName}{' '}
                            {callFnName}
                            {transferSuiTxn}
                        </div>

                        <div className={st.txTransferred}>
                            <div className={st.txAmount}>
                                {formattedAmount} <span>{symbol}</span>
                            </div>
                        </div>
                    </div>

                    {txnsAddress || transferFailed ? (
                        <div className={st.txResult}>
                            {txnsAddress}
                            {transferFailed}
                        </div>
                    ) : null}

                    {txn.url && (
                        <div className={st.txImage}>
                            <img
                                src={txn.url.replace(
                                    /^ipfs:\/\//,
                                    'https://ipfs.io/ipfs/'
                                )}
                                alt={txn?.name || 'NFT'}
                            />
                            <div className={st.nftInfo}>
                                <div className={st.nftName}>
                                    {truncatedNftName}
                                </div>
                                <div className={st.nftDescription}>
                                    {truncatedNftDescription}
                                </div>
                            </div>
                        </div>
                    )}
                    {date && <div className={st.txTypeDate}>{date}</div>}
                </div>
            </div>
        </Link>
    );
}

export default memo(TransactionCard);
