// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useLocation, useParams } from 'react-router-dom';

import ErrorResult from '../../components/error-result/ErrorResult';
import Longtext from '../../components/longtext/Longtext';
import theme from '../../styles/theme.module.css';
import { findDataFromID } from '../../utils/static/searchUtil';

import styles from './TransactionResult.module.css';

type DataType = {
    id: string;
    status: 'success' | 'fail' | 'pending';
    sender: string;
    created?: string[];
    deleted?: string[];
    mutated?: string[];
    recipients: string[];
};

function instanceOfDataType(object: any): object is DataType {
    return (
        object !== undefined &&
        ['id', 'status', 'sender'].every((x) => x in object)
    );
}

function TransactionResult() {
    // TODO - why are we using the location object as state ?
    const { state } = useLocation();
    const { id: txID } = useParams();

    if (process.env.REACT_APP_DATA !== 'static') {
        return (
            <div className={theme.textresults}>
                <div>This page is in Development</div>
            </div>
        );
    }

    const data = findDataFromID(txID, state);

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
            objectIDs = [];
        }

        const statusClass =
            data.status === 'success'
                ? styles['status-success']
                : data.status === 'fail'
                ? styles['status-fail']
                : styles['status-pending'];

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
            <div className={theme.textresults} id="textResults">
                <div>
                    <div>Transaction ID</div>
                    <div id="transactionID">
                        <Longtext
                            text={data.id}
                            category="transactions"
                            isLink={false}
                        />
                    </div>
                </div>

                <div>
                    <div>Status</div>
                    <div id="transactionStatus" className={statusClass}>
                        {data.status}
                    </div>
                </div>

                <div>
                    <div>From</div>
                    <div>
                        <Longtext text={data.sender} category="addresses" />
                    </div>
                </div>

                <div>
                    <div>Event</div>
                    <div className={actionClass}>{action}</div>
                </div>

                <div>
                    <div>Object</div>
                    <div>
                        {objectIDs.map((objectID, index) => (
                            <div key={`object-${index}`}>
                                <Longtext text={objectID} category="objects" />
                            </div>
                        ))}
                    </div>
                </div>

                <div>
                    <div>To</div>
                    <div>
                        {data.recipients.length !== 0 ? (
                            data.recipients.map((address, index) => (
                                <div key={`recipient-${index}`}>
                                    <Longtext
                                        text={address}
                                        category="addresses"
                                    />
                                </div>
                            ))
                        ) : (
                            <div />
                        )}
                    </div>
                </div>
            </div>
        );
    }
    return (
        <ErrorResult
            id={txID}
            errorMsg="There was an issue with the data on the following transaction"
        />
    );
}

export default TransactionResult;
export { instanceOfDataType };
