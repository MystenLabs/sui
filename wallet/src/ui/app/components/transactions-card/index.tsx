// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';

import Icon, { SuiIcons } from '_components/icon';
import { useMiddleEllipsis } from '_hooks';

import type { TxResultState } from '_redux/slices/txresults';

import st from './TransactionsCard.module.scss';

const TRUNCATE_MAX_LENGTH = 8;
const TRUNCATE_PREFIX_LENGTH = 4;

function TransactionCard({ txn }: { txn: TxResultState }) {
    const toAddrStr = useMiddleEllipsis(
        txn.To || '',
        TRUNCATE_MAX_LENGTH,
        TRUNCATE_PREFIX_LENGTH
    );
    const fromAddrStr = useMiddleEllipsis(
        txn.From || '',
        TRUNCATE_MAX_LENGTH,
        TRUNCATE_PREFIX_LENGTH
    );

    const transferStatus = txn.status === 'success' ? 'Checkmark' : 'Close';
    const TxIcon = txn.isSender ? SuiIcons.ArrowLeft : SuiIcons.Buy;
    const iconClassName = txn.isSender
        ? cl(st.arrowActionIcon, st.angledArrow)
        : cl(st.arrowActionIcon, st.buyIcon);

    return (
        <div className={st.card} key={txn.txId}>
            <div className={st.cardIcon}>
                <Icon icon={TxIcon} className={iconClassName} />
            </div>
            <div className={st.cardContent}>
                <div className={st.txResult}>
                    <div className={cl(st.txTypeName, st.kind)}>{txn.kind}</div>
                    {txn?.timestamp_ms && (
                        <div className={st.txTypeDate}>
                            {new Date(txn.timestamp_ms).toDateString()}
                        </div>
                    )}
                </div>
                <div className={st.txResult}>
                    <div className={st.txTypeName}>
                        {txn.kind !== 'Call' ? 'To' : 'From'}:{' '}
                    </div>
                    <div className={cl(st.txValue, st.txAddress)}>
                        {txn.kind !== 'Call' ? toAddrStr : fromAddrStr}
                        <span
                            className={cl(
                                st[txn.status.toLowerCase()],
                                st.txstatus
                            )}
                        >
                            <Icon icon={SuiIcons[transferStatus]} />
                        </span>
                    </div>
                </div>
            </div>
            <div className={st.txTransferred}>
                {txn.Amount && (
                    <>
                        <div className={st.txAmount}>{txn.Amount} SUI</div>
                        <div className={st.txFiatValue}></div>
                    </>
                )}
                {txn.url && (
                    <div className={st.txImage}>
                        <img
                            src={txn.url.replace(
                                /^ipfs:\/\//,
                                'https://ipfs.io/ipfs/'
                            )}
                            alt="NFT"
                        />
                    </div>
                )}
            </div>
        </div>
    );
}

export default memo(TransactionCard);
