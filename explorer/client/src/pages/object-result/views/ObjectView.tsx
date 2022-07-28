// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import ReactJson from 'react-json-view';

import DisplayBox from '../../../components/displaybox/DisplayBox';
import Longtext from '../../../components/longtext/Longtext';
import ModulesWrapper from '../../../components/module/ModulesWrapper';
import OwnedObjects from '../../../components/ownedobjects/OwnedObjects';
import TxForID from '../../../components/transactions-for-id/TxForID';
import theme from '../../../styles/theme.module.css';
import { getOwnerStr, parseImageURL } from '../../../utils/objectUtils';
import { trimStdLibPrefix } from '../../../utils/stringUtils';
import { type DataType } from '../ObjectResultType';
import ObjHeader from './shared/ObjHeader';

import styles from './ObjectView.module.css';

function ObjectView({ data }: { data: DataType }) {
    const IS_MOVE_PACKAGE = data.objType === 'Move Package';

    const checkIsPropertyType = (value: any) =>
        ['number', 'string'].includes(typeof value);

    const stdLibRe = /0x2::/;
    const prepObjTypeValue = (typeString: string) =>
        typeString.replace(stdLibRe, '');

    const viewedData = {
        ...data,
        objType: trimStdLibPrefix(data.objType),
        name: data.name,
        tx_digest: data.data.tx_digest,
        owner: getOwnerStr(data.owner),
        url: parseImageURL(data.data.contents),
    };

    const nameKeyValue = Object.entries(viewedData.data?.contents)
        .filter(([key, _]) => key === 'name')
        .map(([_, value]) => value);

    const properties = Object.entries(viewedData.data?.contents)
        .filter(([key, _]) => key !== 'name')
        .filter(([_, value]) => checkIsPropertyType(value));

    const descriptionTitle = IS_MOVE_PACKAGE
        ? 'Package Description'
        : 'Description';

    const detailsTitle = IS_MOVE_PACKAGE
        ? 'Disassembled Bytecode'
        : 'Properties';

    const isPublisherGenesis =
        data.objType === 'Move Package' && data?.publisherAddress === 'Genesis';

    const structProperties = Object.entries(viewedData.data?.contents)
        .filter(([_, value]) => typeof value == 'object')
        .filter(([key, _]) => key !== 'id');

    let structPropertiesDisplay: any[] = [];
    if (structProperties.length > 0) {
        structPropertiesDisplay = Object.values(structProperties);
    }

    return (
        <>
            <ObjHeader
                data={{
                    objId: data.id,
                    objKind: IS_MOVE_PACKAGE ? 'Package' : 'Object',
                }}
            />
            <div className={styles.resultbox}>
                {viewedData.url !== '' && (
                    <div className={styles.display}>
                        <DisplayBox display={viewedData.url} />
                    </div>
                )}
                <div
                    className={`${styles.textbox} ${
                        viewedData.url
                            ? styles.accommodate
                            : styles.noaccommodate
                    }`}
                >
                    {data.name && <h1>{data.name}</h1>}{' '}
                    {typeof nameKeyValue[0] === 'string' && (
                        <h1>{nameKeyValue}</h1>
                    )}
                    <h2 className={styles.header}>{descriptionTitle}</h2>
                    <div className={theme.textresults} id="descriptionResults">
                        <div>
                            <div>Object ID</div>
                            <div id="objectID">
                                <Longtext
                                    text={data.id}
                                    category="objects"
                                    isLink={false}
                                />
                            </div>
                        </div>
                        {data.data?.tx_digest && !isPublisherGenesis && (
                            <div>
                                <div>Last Transaction ID</div>
                                <div id="lasttxID">
                                    <Longtext
                                        text={data.data?.tx_digest}
                                        category="transactions"
                                        isLink={true}
                                    />
                                </div>
                            </div>
                        )}
                        <div>
                            <div>Version</div>
                            <div>{data.version}</div>
                        </div>
                        {data?.publisherAddress && (
                            <div>
                                <div>Publisher</div>
                                <div id="lasttxID">
                                    <Longtext
                                        text={data.publisherAddress}
                                        category="addresses"
                                        isLink={!isPublisherGenesis}
                                    />
                                </div>
                            </div>
                        )}
                        {data.readonly && (
                            <div>
                                <div>Read Only?</div>
                                {data.readonly === 'true' ? (
                                    <div
                                        id="readOnlyStatus"
                                        className={styles.immutable}
                                    >
                                        True
                                    </div>
                                ) : (
                                    <div
                                        id="readOnlyStatus"
                                        className={styles.mutable}
                                    >
                                        False
                                    </div>
                                )}
                            </div>
                        )}
                        {!IS_MOVE_PACKAGE && (
                            <div>
                                <div>Type</div>
                                <div>{prepObjTypeValue(data.objType)}</div>
                            </div>
                        )}
                        {!IS_MOVE_PACKAGE && (
                            <div>
                                <div>Owner</div>
                                <div id="owner">
                                    <Longtext
                                        text={
                                            typeof viewedData.owner === 'string'
                                                ? viewedData.owner
                                                : typeof viewedData.owner
                                        }
                                        category="unknown"
                                        isLink={
                                            viewedData.owner !== 'Immutable' &&
                                            viewedData.owner !== 'Shared'
                                        }
                                    />
                                </div>
                            </div>
                        )}
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
                    {properties.length > 0 && !IS_MOVE_PACKAGE && (
                        <>
                            <h2 className={styles.header}>{detailsTitle}</h2>
                            <div className={styles.propertybox}>
                                {properties.map(([key, value], index) => (
                                    <div key={`property-${index}`}>
                                        <p>{key}</p>
                                        <p>{value}</p>
                                    </div>
                                ))}
                            </div>
                        </>
                    )}
                    {structProperties.length > 0 &&
                        structPropertiesDisplay.map((itm, index) => (
                            <div key={index}>
                                <div className={styles.propertybox}>
                                    <div>
                                        <p>{itm[0]}</p>
                                    </div>
                                </div>
                                <div className={styles.jsondata}>
                                    <div>
                                        <ReactJson
                                            src={itm[1]}
                                            collapsed={2}
                                            name={false}
                                        />
                                    </div>
                                </div>
                            </div>
                        ))}
                    {!IS_MOVE_PACKAGE ? (
                        <h2 className={styles.header}>Child Objects</h2>
                    ) : (
                        <ModulesWrapper
                            data={{
                                title: 'Modules',
                                content: properties,
                            }}
                        />
                    )}
                    {!IS_MOVE_PACKAGE && (
                        <OwnedObjects id={data.id} byAddress={false} />
                    )}
                    <h2 className={styles.header}>Transactions </h2>
                    <TxForID id={data.id} category="object" />
                </div>
            </div>
        </>
    );
}

export default ObjectView;
