// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useParams } from 'react-router-dom';

import ErrorResult from '../../components/error-result/ErrorResult';
import Longtext from '../../components/longtext/Longtext';
import OwnedObjects from '../../components/ownedobjects/OwnedObjects';
import theme from '../../styles/theme.module.css';

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
            <div className={theme.textresults} id="textResults">
                <div>
                    <div>Address</div>
                    <div id="addressID">
                        <Longtext
                            text={addressID}
                            category="addresses"
                            isLink={false}
                        />
                    </div>
                </div>
                <div>
                    <div>Owned Objects</div>
                    <div>{<OwnedObjects id={addressID} />}</div>
                </div>
            </div>
        );
    } else {
        return <ErrorResult id={addressID} errorMsg={'Something went wrong'} />;
    }
}

export default AddressResult;
export { instanceOfDataType };
