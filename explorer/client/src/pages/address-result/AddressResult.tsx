// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react';
import { useLocation, useParams } from 'react-router-dom';

import ErrorResult from '../../components/error-result/ErrorResult';
import Longtext from '../../components/longtext/Longtext';
import OwnedObjects from '../../components/ownedobjects/OwnedObjects';
import theme from '../../styles/theme.module.css';
import { DefaultRpcClient as rpc } from '../../utils/api/SuiRpcClient';

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

function instanceOfResponseType(input: any): input is ResponseType {
    return input && input.length > 0 && input[0].objectId;
}

function Loaded({ data }: { data: DataType }) {
    return (
        <div className={theme.textresults} id="textResults">
            <div>
                <div>Address ID</div>
                <div id="addressID">
                    <Longtext
                        text={data.id}
                        category="addresses"
                        isLink={false}
                    />
                </div>
            </div>
            <div>
                <div>Owned Objects</div>
                <div>
                    {
                        <OwnedObjects
                            objects={data.objects.map(
                                ({ objectId }) => objectId
                            )}
                        />
                    }
                </div>
            </div>
        </div>
    );
}

function Pending() {
    return <div className={theme.pending}>Please wait for results to load</div>;
}

function Fail({ id }: { id: string | undefined }) {
    return (
        <ErrorResult
            id={id}
            errorMsg="No objects were found for the queried address value"
        />
    );
}

function AddressResultStatic({ addressID }: { addressID: string | undefined }) {
    const { findDataFromID } = require('../../utils/static/searchUtil');
    const data = findDataFromID(addressID, undefined);

    if (instanceOfDataType(data) && instanceOfResponseType(data.objects)) {
        return <Loaded data={data} />;
    } else {
        return <Fail id={addressID} />;
    }
}

function AddressResultInternetAPI({
    addressID,
}: {
    addressID: string | undefined;
}) {
    const defaultData = (addressID: string | undefined) => ({
        id: addressID,
        objects: [{}],
        loadState: 'pending',
    });
    const [data, setData] = useState(defaultData(addressID));

    useEffect(() => {
        if (addressID === undefined) return;

        rpc.getAddressObjects(addressID as string)
            .then((json) => {
                setData({
                    id: addressID,
                    objects: json,
                    loadState: 'loaded',
                });
            })
            .catch((error) => {
                console.log(error);
                setData({ ...defaultData(addressID), loadState: 'fail' });
            });
    }, [addressID]);

    if (
        instanceOfDataType(data) &&
        instanceOfResponseType(data.objects) &&
        data.loadState === 'loaded'
    ) {
        return <Loaded data={data} />;
    }

    if (data.loadState === 'pending') {
        return <Pending />;
    }

    return <Fail id={addressID} />;
}

function AddressResult() {
    const { id: addressID } = useParams();
    const { state } = useLocation();

    if (instanceOfResponseType(state)) {
        const stringid = addressID === undefined ? '' : addressID;
        return (
            <Loaded
                data={{
                    id: stringid,
                    objects: state,
                    loadState: 'loaded',
                }}
            />
        );
    }

    if (process.env.REACT_APP_DATA !== 'static') {
        return <AddressResultInternetAPI addressID={addressID} />;
    } else {
        return <AddressResultStatic addressID={addressID} />;
    }
}

export default AddressResult;
export { instanceOfDataType };
