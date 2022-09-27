// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';
import { useIntl } from 'react-intl';
import { Link } from 'react-router-dom';

import Icon, { SuiIcons } from '_components/icon';
import { formatDate } from '_helpers';
import { useMiddleEllipsis } from '_hooks';
import { GAS_SYMBOL } from '_redux/slices/sui-objects/Coin';
import { balanceFormatOptions } from '_shared/formatting';

import type { TxResultState } from '_redux/slices/txresults';

import st from './TransactionsCard.module.scss';

const TRUNCATE_MAX_LENGTH = 8;
const TRUNCATE_PREFIX_LENGTH = 4;

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

    const intl = useIntl();

    const TransferFailed =
        txn.status !== 'success' ? (
            <div className={st.transferFailed}>Failed</div>
        ) : null;

    //TODO update the logic to account for other transfer type
    const TxIcon = txn.isSender ? SuiIcons.ArrowLeft : SuiIcons.Buy;
    const iconClassName = txn.isSender
        ? cl(st.arrowActionIcon, st.angledArrow)
        : cl(st.arrowActionIcon, st.buyIcon);

    // TODO: update to account for bought, minted, swapped, etc
    const transferType =
        txn.kind !== 'Call' && txn.isSender ? 'Sent' : 'Received';

    const date = txn?.timestampMs
        ? formatDate(txn.timestampMs, ['month', 'day', 'hour', 'minute'])
        : false;

    const TransferSuiTxn = txn.kind === 'TransferSui' ? <span>SUI</span> : null;

    return (
        <Link
            to={`/receipt?${new URLSearchParams({
                txdigest: txn.txId,
            }).toString()}`}
            className={st.txCard}
        >
            <div className={st.card} key={txn.txId}>
                <div className={st.cardIcon}>
                    <Icon icon={TxIcon} className={iconClassName} />
                </div>
                <div className={st.cardContent}>
                    <div className={st.txResult}>
                        <div className={cl(st.txTypeName, st.kind)}>
                            {transferType} {TransferSuiTxn}
                        </div>
                    </div>
                    <div className={st.txResult}>
                        <div className={st.txTypeName}>
                            {txn.kind !== 'Call' && txn.isSender
                                ? 'To'
                                : 'From'}
                        </div>
                        <div className={cl(st.txValue, st.txAddress)}>
                            {txn.kind !== 'Call' && txn.isSender
                                ? toAddrStr
                                : fromAddrStr}
                            <span
                                className={cl(
                                    st[txn.status.toLowerCase()],
                                    st.txstatus
                                )}
                            ></span>
                        </div>
                    </div>
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
                                <div className={st.nftName}>{txn?.name}</div>
                                <div className={st.nftDescription}>
                                    {txn?.description}
                                </div>
                            </div>
                        </div>
                    )}
                    {date && <div className={st.txTypeDate}>{date}</div>}
                </div>
                <div className={st.txTransferred}>
                    <>
                        <div className={st.txAmount}>
                            {intl.formatNumber(
                                BigInt(txn?.amount || txn?.txGas || 0),
                                balanceFormatOptions
                            )}{' '}
                            <span>{GAS_SYMBOL}</span>
                        </div>
                        {TransferFailed}
                    </>
                </div>
            </div>
        </Link>
    );
}

export default memo(TransactionCard);
