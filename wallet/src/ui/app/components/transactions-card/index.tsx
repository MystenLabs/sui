// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';

import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import { useMiddleEllipsis } from '_hooks';

import type { TxResultState } from '_redux/slices/txresults';

import st from './TransactionsCard.module.scss';

function TransactionCard({ txn }: { txn: TxResultState }) {
    const toAddrStr = useMiddleEllipsis(txn.To || '', 20);

    return (
        <div className={st.card} key={txn.txId}>
            <div>
                Tx:{' '}
                <ExplorerLink
                    type={ExplorerLinkType.transaction}
                    transactionID={txn.txId}
                    title="View on Sui Explorer"
                    className={st['explorer-link']}
                >
                    {useMiddleEllipsis(txn.txId || '', 20)}
                </ExplorerLink>
            </div>
            <div>TxType: Call </div>
            <div>
                {' '}
                Gas : {txn.txGas} | Status:{' '}
                <span className={cl(st[txn.status.toLowerCase()], st.txstatus)}>
                    {txn.status === 'success' ? '\u2714' : '\u2716'}{' '}
                </span>
            </div>
            <div>
                From:
                <ExplorerLink
                    type={ExplorerLinkType.address}
                    address={txn.From}
                    title="View on Sui Explorer"
                    className={st.explorerLink}
                >
                    {useMiddleEllipsis(txn.From || '', 20)}
                </ExplorerLink>
            </div>
            {txn?.To && (
                <div>
                    To:
                    <ExplorerLink
                        type={ExplorerLinkType.address}
                        address={txn.To}
                        title="View on Sui Explorer"
                        className={st.explorerLink}
                    >
                        {toAddrStr}
                    </ExplorerLink>
                </div>
            )}
        </div>
    );
}

export default memo(TransactionCard);
