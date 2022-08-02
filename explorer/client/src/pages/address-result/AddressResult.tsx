// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useParams } from 'react-router-dom';

import { ReactComponent as AddressIcon } from '../../assets/AddressIcon.svg';
import ErrorResult from '../../components/error-result/ErrorResult';
import GoBack from '../../components/goback/GoBack';
import Longtext from '../../components/longtext/Longtext';
import OwnedObjects from '../../components/ownedobjects/OwnedObjects';
import TxForID from '../../components/transactions-for-id/TxForID';

import styles from './AddressResult.module.css';

type DataType = {
    id: string;
    objects: ResponseType;
    loadState?: 'loaded' | 'pending' | 'fail';
};

type ResponseType = {
    objectId: string;
}[];

function instanceOfDataType(object: any): object is DataType {
    return object !== undefined && ['id', 'objects'].every((x) => x in object);
}

function AddressResult() {
    const { id: addressID } = useParams();

    if (addressID !== undefined) {
        return (
            <>
                <GoBack />
                <div className={styles.results} id="textResults">
                    <div className={styles.addressid}>
                        <span>
                            <AddressIcon /> Address
                        </span>
                        <div id="addressID">
                            <Longtext
                                text={addressID}
                                category="addresses"
                                isLink={false}
                            />
                        </div>
                    </div>
                    <div>
                        <h1>Owned Objects</h1>
                        <div>
                            {<OwnedObjects id={addressID} byAddress={true} />}
                        </div>
                    </div>
                    <div>
                        <h1>Transactions</h1>
                        <TxForID id={addressID} category="address" />
                    </div>
                </div>
            </>
        );
    } else {
        return <ErrorResult id={addressID} errorMsg={'Something went wrong'} />;
    }
}

export default AddressResult;
export { instanceOfDataType };
