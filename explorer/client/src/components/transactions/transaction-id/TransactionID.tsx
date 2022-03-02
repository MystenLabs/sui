import { memo } from 'react';
import { Link } from 'react-router-dom';

import TruncatedLabel from '../../truncated-label/TruncatedLabel';

type TransactionIDProps = {
    id: string;
};

function TransactionID({ id }: TransactionIDProps) {
    return (
        <Link to={`/transactions/${id}`}>
            <TruncatedLabel label={id} />
        </Link>
    );
}

export default memo(TransactionID);
