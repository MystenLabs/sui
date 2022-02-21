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
    return object && 
    ['id', 'status', 'sender'].every( x => x in object) && 
    ['created', 'deleted', 'mutated'].some(x => x in object);
}

function TransactionResult() {
    const { id: txID } = useParams();
    const data = mockTransactionData.data.find(({ id }) => id === txID);

    if (instanceOfDataType(data)) {
        let action;
        let objectID;

        if (data) {
            if ('created' in data) {
                action = 'Create';
                objectID = data?.created?.[0];
            } else if ('deleted' in data) {
                action = 'Delete';
                objectID = data?.deleted?.[0];
            } else if ('mutated' in data) {
                action = 'Mutate';
                objectID = data?.mutated?.[0];
            } else {
                action = 'Unknown';
                objectID = 'Unknown';
            }
        } else {
            action = 'No Data';
            objectID = 'No Data';
        }

        return (
            <div className={styles.data}>
                <div className={styles.label}>Transaction ID</div>
                <div className={styles.result}>{data.id}</div>

                <div className={styles.label}>Status</div>
                <div className={styles.result}>{data.status}</div>

                <div className={styles.label}>Sender</div>
                <div className={styles.result}>{data.sender}</div>

                <div className={styles.label}>Did</div>
                <div className={styles.result}>{action}</div>

                <div className={styles.label}>Object</div>
                <div className={styles.result}>{objectID}</div>
            </div>
        );
    }
    return (
        <div className={styles.data}>
            <div className={styles.label}>
                This transaction could not be found:
            </div>
            <div className={styles.result}>{txID}</div>
        </div>
    );
}

export default TransactionResult;
