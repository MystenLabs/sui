import cn from 'classnames';
import { memo } from 'react';

import AccountID from '../../accounts/account-id/AccountID';
import ObjectID from '../../objects/object-id/ObjectID';
import TransactionID from '../transaction-id/TransactionID';
import TransactionStatus from '../transaction-status/TransactionStatus';
import styles from './TransactionsTable.module.css';

import type { TransactionType } from '../types';

type TransactionsTableProps = {
    transactions: TransactionType[];
};

const headers = [
    'Transaction ID',
    'Sender',
    'Status',
    'Objects Created',
    'Objects Mutated',
    'Objects Deleted',
];

const clsColumn = styles.column;
const OBJECTS_LIMIT = 2;

function makeObjects(objs: string[]) {
    return (
        <td className={cn(clsColumn, styles['column-objs'])}>
            {objs.length ? (
                <ul className={styles.objects}>
                    {objs.slice(0, OBJECTS_LIMIT).map((id) => (
                        <li key={id} className={styles.object}>
                            <ObjectID id={id} size="small" />
                        </li>
                    ))}
                    {objs.length > OBJECTS_LIMIT ? (
                        <li className={styles.object}>
                            <span className={styles.extra}>
                                +{objs.length - OBJECTS_LIMIT} more
                            </span>
                        </li>
                    ) : null}
                </ul>
            ) : (
                '-'
            )}
        </td>
    );
}

function TransactionsTable({ transactions }: TransactionsTableProps) {
    return (
        <div className={styles['table-container']}>
            <table className={styles['table']}>
                <thead>
                    <tr>
                        {headers.map((txt) => (
                            <th key={txt} className={clsColumn}>
                                {txt}
                            </th>
                        ))}
                    </tr>
                </thead>
                <tbody>
                    {transactions.map((tx) => (
                        <tr key={tx.id} className={styles.row}>
                            <td className={clsColumn}>
                                <TransactionID id={tx.id} />
                            </td>
                            <td className={clsColumn}>
                                <AccountID id={tx.sender} />
                            </td>
                            <td className={clsColumn}>
                                <TransactionStatus status={tx.status} />
                            </td>
                            {makeObjects(tx.created || [])}
                            {makeObjects(tx.mutated || [])}
                            {makeObjects(tx.deleted || [])}
                        </tr>
                    ))}
                </tbody>
            </table>
        </div>
    );
}

export default memo(TransactionsTable);
