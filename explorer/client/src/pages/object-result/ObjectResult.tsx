//import 'ace-builds/src-noconflict/theme-github';
import React, { useEffect, useState, useCallback, useRef } from 'react';
//import AceEditor from 'react-ace';
import { useParams } from 'react-router-dom';

import ErrorResult from '../../components/error-result/ErrorResult';
import Longtext from '../../components/longtext/Longtext';
import theme from '../../styles/theme.module.css';
import { type AddressOwner, SuiRpcClient, DefaultRpcClient } from '../../utils/rpc';
import { asciiFromNumberBytes, trimStdLibPrefix} from '../../utils/utility_functions';

import styles from './ObjectResult.module.css';

type DataType = {
    id: string;
    category: string;
    owner: string | AddressOwner;
    version: string;
    readonly?: string;
    objType: string;
    name?: string;
    ethAddress?: string;
    ethTokenId?: string;
    contract_id?: { bytes: string };
    data: {
        contents: {
            [key: string]: any;
        };
        owner?: { AddressOwner: number[] } | string,
        tx_digest?: number[] | string
    };
   loadState?: string;
};

const DATATYPE_DEFAULT: DataType = {
    id: '',
    category: '',
    owner: '',
    version: '',
    objType: '',
    data: { contents: {} },
    loadState: 'pending'
}

// TODO - restore or remove this functionality
const IS_SMART_CONTRACT = (data: DataType) => false;

//TODO - create more comprehensive check thatresults from API are as expected:
/*
function instanceOfDataType(object: any) {
    return (
        object !== undefined &&
        ['id', 'version', 'objType'].every((x) => x in object) &&
        object['id'].length > 0
    );
}
*/


const _rpc = DefaultRpcClient;
console.log(_rpc);

type SuiIdBytes = { bytes: number[] };

function handleSpecialDemoNameArrays(data: {
    name?: SuiIdBytes | string,
    player_name?: SuiIdBytes | string,
    monster_name?: SuiIdBytes | string,
    farm_name?: SuiIdBytes | string
}): string
{
    let bytesObj: SuiIdBytes = { bytes: [] };

    if('player_name' in data) {
        bytesObj = data.player_name as SuiIdBytes;
        const ascii = asciiFromNumberBytes(bytesObj.bytes);
        delete data.player_name;
        return ascii;
    }
    else if('monster_name' in data) {
        bytesObj = data.monster_name as SuiIdBytes;
        const ascii = asciiFromNumberBytes(bytesObj.bytes);
        delete data.monster_name;
        return ascii;
    }
    else if('farm_name' in data) {
        bytesObj = data.farm_name as SuiIdBytes;
        const ascii = asciiFromNumberBytes(bytesObj.bytes);
        delete data.farm_name;
        return ascii;
    }
    else if('name' in data) {
        bytesObj = data.name as SuiIdBytes;
        return asciiFromNumberBytes(bytesObj.bytes);
    }
    else
        bytesObj = { bytes: [] };

    return asciiFromNumberBytes(bytesObj.bytes);
}

function DisplayBox({ data }: { data: DataType }) {
    const [hasDisplayLoaded, setHasDisplayLoaded] = useState(false);

    const imageStyle = hasDisplayLoaded ? {} : { display: 'none' };

    const handleImageLoad = useCallback(
        () => setHasDisplayLoaded(true),
        [setHasDisplayLoaded]
    );

    if (data.data.contents.display) {
        if(typeof data.data.contents.display === 'object' && 'bytes' in data.data.contents.display)
            data.data.contents.display = asciiFromNumberBytes(data.data.contents.display.bytes);

        return (
            <div className={styles['display-container']}>
                {!hasDisplayLoaded && (
                    <div className={styles.imagebox}>
                        Please wait for display to load
                    </div>
                )}
                <img
                    className={styles.imagebox}
                    style={imageStyle}
                    alt="NFT"
                    src={data.data.contents.display}
                    onLoad={handleImageLoad}
                />
            </div>
        );
    }

    if (IS_SMART_CONTRACT(data)) {
        return (
            <div>Smart Contract Support is coming soon</div>
          /* TODO - implement smart contract support with real data
            <div className={styles['display-container']}>
                <AceEditor
                    theme="github"
                    value={data.data.contents.display}
                    showGutter={true}
                    readOnly={true}
                    fontSize="0.8rem"
                    className={styles.codebox}
                />
            </div>
            */
        );
    }

    return <div />;
}



async function getObjectState(objID: string): Promise<object> {
    /*
    return new Promise((resolve, reject) => {
        let data = findDataFromID(objID, {});
        if (data) resolve(data);
        else reject('object ID not found');
    });
    */
    return _rpc.getObjectInfo(objID);
}

function toHexString(byteArray: number[]): string {
    return '0x' +
        Array.prototype.map.call(byteArray, (byte) => {
            return ('0' + (byte & 0xFF).toString(16)).slice(-2);
        })
        .join('');
}

const ObjectResult = ((): JSX.Element => {
    const { id: objID } = useParams();

    const [showDescription, setShowDescription] = useState(true);
    const [showProperties, setShowProperties] = useState(false);
    const [showConnectedEntities, setShowConnectedEntities] = useState(false);

    const [showObjectState, setObjectState] = useState(DATATYPE_DEFAULT);

    const prepLabel = (label: string) => label.split('_').join(' ');
    const checkIsPropertyType = (value: any) =>
        ['number', 'string'].includes(typeof value);

    const checkIsIDType = (key: string, value: any) =>
        /owned/.test(key) || (/_id/.test(key) && value?.bytes) || value?.vec;
    const checkSingleID = (value: any) => value?.bytes;
    const checkVecIDs = (value: any) => value?.vec;
    
    //TODO - improve move code handling:
    // const isMoveVecType = (value: { vec?: [] }) => Array.isArray(value?.vec);

    const extractOwnerData = (owner: string | AddressOwner): string => {
        switch (typeof(owner)) {
            case 'string':
                if(addrOwnerPattern.test(owner)) {
                    let ownerId = getAddressOwnerId(owner);
                    return ownerId ? ownerId : '';
                }
                const singleOwnerPattern = /SingleOwner\(k#(.*)\)/;
                const result = singleOwnerPattern.exec(owner);
                return result ? result[1] : '';
            case 'object':
                if('AddressOwner' in owner) {
                    let ownerId = extractAddressOwner(owner.AddressOwner);
                    return ownerId ? ownerId : '';
                }
                return '';
            default:
                return '';
        }
    };

    const addrOwnerPattern = /^AddressOwner\(k#/;
    const endParensPattern = /\){1}$/
    const getAddressOwnerId = (addrOwner: string): string | null => {
        if (!addrOwnerPattern.test(addrOwner) || !endParensPattern.test(addrOwner))
            return null;

        let str = addrOwner.replace(addrOwnerPattern, '');
        return str.replace(endParensPattern, '');
    };

    const extractAddressOwner = (addrOwner: number[]): string | null => {
        if(addrOwner.length !== 20) {
            console.log('address owner byte length must be 20');
            return null;
        }

        return asciiFromNumberBytes(addrOwner);
    };

    let dataRef = useRef(DATATYPE_DEFAULT);

    useEffect(() => {
        getObjectState(objID as string)
        .then((objState) => {
                let asType = objState as DataType;
                setObjectState({...asType, loadState: 'loaded'} );
                dataRef.current = asType;
        })
        .catch((error) => {
          console.log(error);
          setObjectState({...DATATYPE_DEFAULT, loadState: 'fail'})
        } )
      ;
    }, [objID]);

    // TODO - merge / replace with other version of same thing
    const stdLibRe = /0x2::/;
    const prepObjTypeValue = (typeString: string) =>
        typeString.replace(stdLibRe, '');

    useEffect(() => {
        setShowDescription(true);
        setShowProperties(true);
        setShowConnectedEntities(true);
    }, [setShowDescription, setShowProperties, setShowConnectedEntities]);

    if (showObjectState.loadState === 'loaded') {
        let data = showObjectState;
        const innerData = data.data;

        data = SuiRpcClient.modifyForDemo(data);
        data.objType = trimStdLibPrefix(data.objType);

        // hardcode a friendly name for gas for now
        const gasTokenTypeStr = 'Coin::Coin<0x2::GAS::GAS>';
        const gasTokenId = '0000000000000000000000000000000000000003';
        if (data.objType === gasTokenTypeStr && data.id === gasTokenId)
            data.name = 'GAS';

        if(!data.name)
            data.name = handleSpecialDemoNameArrays(innerData.contents);

        if(innerData.tx_digest && typeof(innerData.tx_digest) === 'object') {
            const digest_hex = toHexString(innerData.tx_digest as number[]);
            innerData.tx_digest = digest_hex;
        }

        switch (typeof(innerData.owner)) {
            case 'object':
                const ownerObj = innerData.owner as object;
                if ('AddressOwner' in ownerObj) {
                    innerData.owner = toHexString((ownerObj as AddressOwner).AddressOwner);
                    console.log(innerData);
                }
                break;
        }

        //TO DO remove when have distinct name field under Description
        const nameKeyValue = Object.entries(innerData?.contents)
            .filter(([key, value]) => /name/i.test(key))
            .map(([key, value]) => value);

        const ownedObjects = Object.entries(innerData.contents).filter(
            ([key, value]) => checkIsIDType(key, value)
        );
        const properties = Object.entries(innerData.contents)
            //TO DO: remove when have distinct 'name' field in Description
            .filter(([key, value]) => !/name/i.test(key))
            .filter(([_, value]) => checkIsPropertyType(value))
            .filter(([key, _]) => key !== 'display');

        return (<>
            <div className={styles.resultbox}>
                {data?.data.contents.display && (
                    <DisplayBox data={data} />
                )}
                <div
                    className={`${styles.textbox} ${
                        data?.data.contents.display
                            ? styles.accommodate
                            : styles.noaccommodate
                    }`}
                >
                    {data.name && <h1>{data.name}</h1>} {' '}
                    {typeof nameKeyValue[0] === 'string' && (
                        <h1>{nameKeyValue}</h1>
                    )}
                    <h2
                        className={styles.clickableheader}
                        onClick={() => setShowDescription(!showDescription)}
                    >
                        Description {showDescription ? '' : '+'}
                    </h2>
                    {showDescription && (
                        <div className={theme.textresults}>
                            <div>
                                <div>Object ID</div>
                                <div>
                                    <Longtext
                                        text={data.id}
                                        category="objects"
                                        isLink={false}
                                    />
                                </div>
                            </div>

                            <div>
                                <div>Version</div>
                                <div>{data.version}</div>
                            </div>

                            {data.readonly && (
                                <div>
                                    <div>Read Only?</div>
                                    {data.readonly === 'true' ? (
                                        <div
                                            data-testid="read-only-text"
                                            className={styles.immutable}
                                        >
                                            True
                                        </div>
                                    ) : (
                                        <div
                                            data-testid="read-only-text"
                                            className={styles.mutable}
                                        >
                                            False
                                        </div>
                                    )}
                                </div>
                            )}

                            <div>
                                <div>Type</div>
                                <div>{prepObjTypeValue(data.objType)}</div>
                            </div>
                            <div>
                                <div>Owner</div>
                                <Longtext
                                    text={extractOwnerData(data.owner)}
                                    category="unknown"
                                    isLink={true}
                                />
                            </div>
                            {data.contract_id && (
                                <div>
                                    <div>Contract ID</div>
                                    <Longtext
                                        text={data.contract_id.bytes}
                                        category="objects"
                                        isLink={true}
                                    />
                                </div>
                            )}

                            {data.ethAddress && (
                                <div>
                                    <div>Ethereum Contract Address</div>
                                    <div>
                                        <Longtext
                                            text={data.ethAddress}
                                            category="ethAddress"
                                            isLink={true}
                                        />
                                    </div>
                                </div>
                            )}
                            {data.ethTokenId && (
                                <div>
                                    <div>Ethereum Token ID</div>
                                    <div>
                                        <Longtext
                                            text={data.ethTokenId}
                                            category="addresses"
                                            isLink={false}
                                        />
                                    </div>
                                </div>
                            )}
                        </div>
                    )}
                    {!IS_SMART_CONTRACT(data) && properties.length > 0 && (
                        <>
                            <h2
                                className={styles.clickableheader}
                                onClick={() =>
                                    setShowProperties(!showProperties)
                                }
                            >
                                Properties {showProperties ? '' : '+'}
                            </h2>
                            {showProperties && (
                                <div className={styles.propertybox}>
                                    {properties.map(([key, value], index) => (
                                        <div key={`property-${index}`}>
                                            <p>{prepLabel(key)}</p>
                                            <p>{value}</p>
                                        </div>
                                    ))}
                                </div>
                            )}
                        </>
                    )}
                    {ownedObjects.length > 0 && (
                        <>
                            <h2
                                className={styles.clickableheader}
                                onClick={() =>
                                    setShowConnectedEntities(
                                        !showConnectedEntities
                                    )
                                }
                            >
                                Owned Objects {showConnectedEntities ? '' : '+'}
                            </h2>
                            {showConnectedEntities && (
                                <div className={theme.textresults}>
                                    {ownedObjects.map(
                                        ([key, value], index1) => (
                                            <div
                                                key={`ConnectedEntity-${index1}`}
                                            >
                                                <div>{prepLabel(key)}</div>
                                                {checkSingleID(value) && (
                                                    <Longtext
                                                        text={value.bytes}
                                                        category="objectId"
                                                    />
                                                )}
                                                {checkVecIDs(value) && (
                                                    <div>
                                                        {value?.vec.map(
                                                            (
                                                                value2: {
                                                                    bytes: string;
                                                                },
                                                                index2: number
                                                            ) => (
                                                                <Longtext
                                                                    text={
                                                                        value2.bytes
                                                                    }
                                                                    category="objectId"
                                                                    key={`ConnectedEntity-${index1}-${index2}`}
                                                                />
                                                            )
                                                        )}
                                                    </div>
                                                )}
                                            </div>
                                        )
                                    )}
                                </div>
                            )}
                        </>
                    )}
                </div>
            </div></>
        );
    }
    if (showObjectState.loadState === 'pending') {
      return <div className={theme.pending}>Please wait for results to load</div>;
    }
    if (showObjectState.loadState === 'fail') {
    return (
        <ErrorResult
            id={objID}
            errorMsg="There was an issue with the data on the following object"
        />
    );
    }

    return <div>"Something went wrong"</div>;

});


export { ObjectResult };
export type { DataType };
