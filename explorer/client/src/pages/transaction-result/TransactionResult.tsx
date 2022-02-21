import { useParams } from 'react-router-dom';

import mockTransactionData from '../../utils/transaction_mock.json';
import styles from './TransactionResult.module.css';

type DataType = {
    id: string;
    status: string;
    sender: string;
    created?: string[];
    deleted?: string[];
    mutated?: string[];
};

function instanceOfDataType(object: any): object is DataType {
    return (
        object !== undefined &&
        ['id', 'status', 'sender'].every((x) => x in object)
    );
}

function TransactionResult() {
    const { id: txID } = useParams();
    const data = mockTransactionData.data.find(({ id }) => id === txID);

    if (instanceOfDataType(data)) {
        let action;
        let objectID;

        if (data.created !== undefined) {
            action = 'Create';
            objectID = data.created[0];
        } else if (data.deleted !== undefined) {
            action = 'Delete';
            objectID = data.deleted[0];
        } else if (data.mutated !== undefined) {
            action = 'Mutate';
            objectID = data.mutated[0];
        } else {
            action = 'Fail';
            objectID = '-';
        }

        return (
            <dl className={styles.data}>
                <dt>Transaction ID</dt>
                <dd>{data.id}</dd>

                <dt>Status</dt>
                <dd data-testid="transaction-status">
                    {data.status}
                </dd>

                <dt>Sender</dt>
                <dd>{data.sender}</dd>

                <dt>Did</dt>
                <dd>{action}</dd>

                <dt>Object</dt>
                <dd>{objectID}</dd>
            </dl>
        );
    }
    return (
        <dl className={styles.data}>
            <dt>
                This transaction could not be found:
            </dt>
            <dd>
               {txID}
            </dd>
        </dl>
    );
}

export default TransactionResult;
export { instanceOfDataType };
