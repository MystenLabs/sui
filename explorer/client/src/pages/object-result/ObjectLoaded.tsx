// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState, useCallback } from 'react';

import DisplayBox from '../../components/displaybox/DisplayBox';
import Longtext from '../../components/longtext/Longtext';
import OwnedObjects from '../../components/ownedobjects/OwnedObjects';
import TxForID from '../../components/transactions-for-id/TxForID';
import codestyle from '../../styles/bytecode.module.css';
import theme from '../../styles/theme.module.css';
import { getOwnerStr, parseImageURL } from '../../utils/objectUtils';
import { trimStdLibPrefix } from '../../utils/stringUtils';
import { type DataType } from './ObjectResultType';

import styles from './ObjectResult.module.css';

function ObjectLoaded({ data }: { data: DataType }) {
    // TODO - restore or remove this functionality
    const [showDescription, setShowDescription] = useState(true);
    const [showProperties, setShowProperties] = useState(false);
    const [showConnectedEntities, setShowConnectedEntities] = useState(false);
    const [showTx, setShowTx] = useState(true);

    useEffect(() => {
        setShowDescription(true);
        setShowProperties(true);
        setShowConnectedEntities(true);
        setShowTx(true);
    }, [
        setShowDescription,
        setShowProperties,
        setShowConnectedEntities,
        setShowTx,
    ]);

    const clickSetShowDescription = useCallback(
        () => setShowDescription(!showDescription),
        [showDescription]
    );
    const clickSetShowProperties = useCallback(
        () => setShowProperties(!showProperties),
        [showProperties]
    );
    const clickSetShowConnectedEntities = useCallback(
        () => setShowConnectedEntities(!showConnectedEntities),
        [showConnectedEntities]
    );
    const clickSetShowTx = useCallback(() => setShowTx(!showTx), [showTx]);
    const prepLabel = (label: string) => label.split('_').join(' ');
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

    const descriptionTitle =
        data.objType === 'Move Package' ? 'Package Description' : 'Description';

    const detailsTitle =
        data.objType === 'Move Package'
            ? 'Disassembled Bytecode'
            : 'Properties';

    const isPublisherGenesis =
        data.objType === 'Move Package' && data?.publisherAddress === 'Genesis';

    return (
        <>
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
                    <h2
                        className={styles.clickableheader}
                        onClick={clickSetShowDescription}
                    >
                        {descriptionTitle} {showDescription ? '' : '+'}
                    </h2>
                    {showDescription && (
                        <div
                            className={theme.textresults}
                            id="descriptionResults"
                        >
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
                            {data.objType !== 'Move Package' && (
                                <div>
                                    <div>Type</div>
                                    <div>{prepObjTypeValue(data.objType)}</div>
                                </div>
                            )}
                            {data.objType !== 'Move Package' && (
                                <div>
                                    <div>Owner</div>
                                    <div id="owner">
                                        <Longtext
                                            text={
                                                typeof viewedData.owner ===
                                                'string'
                                                    ? viewedData.owner
                                                    : typeof viewedData.owner
                                            }
                                            category="unknown"
                                            isLink={
                                                viewedData.owner !==
                                                    'Immutable' &&
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
                    )}
                    {properties.length > 0 && data.objType !== 'Move Package' && (
                        <>
                            <h2
                                className={styles.clickableheader}
                                onClick={clickSetShowProperties}
                            >
                                {detailsTitle} {showProperties ? '' : '+'}
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
                    {}
                    {data.objType !== 'Move Package' ? (
                        <h2
                            className={styles.clickableheader}
                            onClick={clickSetShowConnectedEntities}
                        >
                            Child Objects {showConnectedEntities ? '' : '+'}
                        </h2>
                    ) : (
                        <div>
                            <h2
                                className={styles.clickableheader}
                                onClick={clickSetShowProperties}
                            >
                                Modules {showProperties ? '' : '+'}
                            </h2>
                            {showProperties && (
                                <div className={styles.bytecodebox}>
                                    {properties.map(([key, value], index) => (
                                        <div key={`property-${index}`}>
                                            <div>{prepLabel(key)}</div>
                                            <div className={codestyle.code}>
                                                {value}
                                            </div>
                                        </div>
                                    ))}
                                </div>
                            )}
                        </div>
                    )}
                    {showConnectedEntities &&
                        data.objType !== 'Move Package' && (
                            <OwnedObjects id={data.id} byAddress={false} />
                        )}
                    <h2
                        className={styles.clickableheader}
                        onClick={clickSetShowTx}
                    >
                        Transactions {showTx ? '' : '+'}
                    </h2>
                    {showTx && <TxForID id={data.id} category="object" />}
                </div>
            </div>
        </>
    );
}

export default ObjectLoaded;
