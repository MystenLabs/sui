import { memo } from 'react';

import { TransactionStatus as TxStatus } from '../types';

type TransactionStatusProps = {
    status: TxStatus;
};

const statusToLabel = {
    [TxStatus.success]: '✔ Success',
    [TxStatus.fail]: '✕ Fail',
};

function TransactionStatus({ status }: TransactionStatusProps) {
    return <span>{statusToLabel[status]}</span>;
}

export default memo(TransactionStatus);
