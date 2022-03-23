import { useEffect, useState } from 'react';
import { useParams } from 'react-router-dom';

import ErrorResult from '../../components/error-result/ErrorResult';
import Longtext from '../../components/longtext/Longtext';
import theme from '../../styles/theme.module.css';
import { DefaultRpcClient } from '../../utils/rpc';


type DataType = {
    id: string;
    objects: {
        objectId: string,
        version: string,
        objectDigest: string
    }[];
};

function instanceOfDataType(object: any): object is DataType {
    return object !== undefined && ['id', 'objects'].every((x) => x in object);
}

function AddressResult() {
    const rpc = DefaultRpcClient;
    const { id: addressID } = useParams();
    const defaultData = (addressID: string | undefined) => ({ id: addressID, objects: [{}], loadState: 'pending' });
    const [data, setData] = useState(defaultData(addressID));

    useEffect(() => {
        if(addressID === undefined)
            return;

        rpc.getAddressObjects(addressID)
        .then((json) => {
            console.log(json);
            setData(
              {
                id: addressID,
                objects: json,
                loadState: 'loaded'
              } 
            )
        })
      .catch((error) => {
        setData({...defaultData(addressID), loadState: 'fail'})
      })
      ;
    }, [addressID, rpc]);

    if (instanceOfDataType(data) && data.loadState === 'loaded') {
        return (
            <div className={theme.textresults}>
                <div>
                    <div>Address ID</div>
                    <div>
                        <Longtext
                            text={data?.id}
                            category="addresses"
                            isLink={false}
                        />
                    </div>
                </div>
                <div>
                    <div>Owned Objects</div>
                    <div>
                        {data.objects.map(
                            (objectID: { objectId: string; }, index: any) => (
                            <div key={`object-${index}`}>
                                <Longtext
                                    text={objectID.objectId}
                                    category="objects"
                                />
                            </div>
                        ))}
                    </div>
                </div>
            </div>
        );
    }
  if (data.loadState === 'pending'){
    return <div className={theme.pending}>Please wait for results to load</div>;
  }
  if (data.loadState === 'fail'){
    return (
        <ErrorResult
            id={addressID}
            errorMsg="There was an issue with the data on the following address"
        />
    );
  }
  return <div>Something went wrong</div>;
}

export default AddressResult;
export { instanceOfDataType };
