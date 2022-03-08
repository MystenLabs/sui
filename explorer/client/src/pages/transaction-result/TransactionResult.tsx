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
        let action: string;
        let objectIDs: string[];

        if (data.created !== undefined) {
            action = 'Create';
            objectIDs = data.created;
        } else if (data.deleted !== undefined) {
            action = 'Delete';
            objectIDs = data.deleted;
        } else if (data.mutated !== undefined) {
            action = 'Mutate';
            objectIDs = data.mutated;
        } else {
            action = 'Fail';
            objectIDs = ['-'];
        }

        const statusClass =
            data.status === 'success'
                ? styles['status-success']
                : styles['status-fail'];

        let actionClass;

        switch (action) {
            case 'Create':
                actionClass = styles['action-create'];
                break;
            case 'Delete':
                actionClass = styles['action-delete'];
                break;
            case 'Fail':
                actionClass = styles['status-fail'];
                break;
            default:
                actionClass = styles['action-mutate'];
        }

        return (
            <dl className={styles.data}>
                <dt>Transaction ID</dt>
                <dd>{data.id}</dd>

                <dt>Status</dt>
                <dd data-testid="transaction-status" className={statusClass}>
                    {data.status}
                </dd>

                <dt>Sender</dt>
                <dd>{data.sender}</dd>

                <dt>Did</dt>
                <dd className={actionClass}>{action}</dd>

                <dt>Object</dt>
                {objectIDs.map((objectID, index) => (
                    <dd key={`object-${index}`}>{objectID}</dd>
                ))}
            </dl>
        );
    }
    return (
        <dl className={styles.data}>
            <dt>This transaction could not be found:</dt>
            <dd>{txID}</dd>
        </dl>
    );
}

export default TransactionResult;
export { instanceOfDataType };
