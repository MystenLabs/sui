import { Base64DataBuffer } from '@mysten/sui.js';
import cl from 'classnames';
import { useState, useContext, useEffect } from 'react';
import { useLocation } from 'react-router-dom';

import ErrorResult from '../../components/error-result/ErrorResult';
import { NetworkContext } from '../../context';
import theme from '../../styles/theme.module.css';
import {
    type DataType,
    getObjectDataWithPackageAddress,
} from '../object-result/ObjectResult';

import type { Validator, ValidatorMetadata, ValidatorState } from '../../components/top-validators-card/TopValidatorsCard';

import objStyles from '../object-result/ObjectResult.module.css';
import txStyles from '../transaction-result/TransactionResult.module.css';



const VALIDATORS_OBJECT_ID = '0x05';

const DATATYPE_DEFAULT = { loadState: 'pending' };

const textDecoder = new TextDecoder();

function ValidatorMetadataElement({
    meta,
}: {
    meta: ValidatorMetadata;
}): JSX.Element {
    if (!meta) return <></>;

    console.log('meta', meta);

    const name = meta ? meta.fields.name : 'unknown';
    const addr = meta ? meta.fields.sui_address : 'unknown';
    const pubkey = meta ? meta.fields.pubkey_bytes : '';

    return (
        <div>
            <h3>{textDecoder.decode(new Base64DataBuffer(name).getData())}</h3>
            <h4>Address</h4>
            {addr}
            <h4>Pubkey</h4>
            {pubkey}
        </div>
    );
}

function ValidatorElement({ itm }: { itm: Validator }): JSX.Element {
    if (!itm.fields.metadata) return <></>;

    console.log('meta', itm.fields.metadata);

    const name = itm.fields.metadata
        ? itm.fields.metadata.fields.name
        : 'unknown';
    const addr = itm.fields.metadata
        ? itm.fields.metadata.fields.sui_address
        : 'unknown';
    const pubkey = itm.fields.metadata
        ? itm.fields.metadata.fields.pubkey_bytes
        : '';
    return (
        <div>
            <h3>{textDecoder.decode(new Base64DataBuffer(name).getData())}</h3>
            <h4>Stake</h4>
            {itm.fields.stake_amount} SUI
            <h4>Address</h4>
            {addr}
            <h4>Pubkey</h4>
            {pubkey}
            <h5>Delegation</h5>
            {itm.fields.delegation}
            <h5>Delegation Count</h5>
            {itm.fields.delegation_count ? itm.fields.delegation_count : 0}
            <h5>Pending Delegation</h5>
            {itm.fields.pending_delegation}
            <h5>Pending Delegation Withdraw</h5>
            {itm.fields.pending_delegation_withdraw}
            <div>
                <div>
                    <h5>Pending Delegators</h5>
                    {itm.fields.pending_delegator_count}
                </div>
                <div>
                    <h5>Pending Delegator Withdraws</h5>
                    {itm.fields.pending_delegator_withdraw_count}
                </div>
            </div>
        </div>
    );
}

function ValidatorObjectLoaded({ data }: { data: DataType }): JSX.Element {
    console.log('validator object loaded', data);

    const contents = data.data['contents'] as ValidatorState;
    console.log(contents);

    const active_set = contents.validators.fields.active_validators;
    const next_epoch_set = contents.validators.fields.next_epoch_validators;

    const totalStake = contents.validators.fields.validator_stake;
    const quorumStake = contents.validators.fields.quorum_stake_threshold;

    return (
        <>
            <div id="validators">
                <h1 className={objStyles.clickableheader}>Validators</h1>

                <div className={txStyles.txcard}>
                    <h3>Total Stake</h3>
                    {totalStake}
                    <h3>Qourum Stake</h3>
                    {quorumStake}
                </div>

                <div id="activeset" className={txStyles.txcard}>
                    <h2>Active</h2>

                    {active_set.map((itm: any, i: number) => (
                        <div
                            key={i}
                            className={cl(
                                txStyles.txcardgrid,
                                itm.className ? txStyles[itm.className] : ''
                            )}
                        >
                            <ValidatorElement itm={itm} />
                            <br />
                        </div>
                    ))}
                </div>

                <div id="nextepochset" className={txStyles.txcard}>
                    <h2>Next Epoch</h2>

                    {next_epoch_set.map((itm: any, i: number) => (
                        <div
                            key={i}
                            className={cl(
                                txStyles.txcardgrid,
                                itm.className ? txStyles[itm.className] : ''
                            )}
                        >
                            <ValidatorMetadataElement meta={itm} />
                            <br />
                        </div>
                    ))}
                </div>

                <div id="sysparams" className={txStyles.txcard}>
                    <h2>System Parameters</h2>
                </div>
                <br />
            </div>
        </>
    );
}

const ValidatorsResultAPI = (): JSX.Element => {
    const [showObjectState, setObjectState] = useState(DATATYPE_DEFAULT);
    const [network] = useContext(NetworkContext);
    useEffect(() => {
        getObjectDataWithPackageAddress(VALIDATORS_OBJECT_ID, network)
            .then((objState: any) => {
                console.log('validator state', objState);
                setObjectState({
                    ...(objState as DataType),
                    loadState: 'loaded',
                });
            })
            .catch((error: any) => {
                console.log(error);
                setObjectState({ ...DATATYPE_DEFAULT, loadState: 'fail' });
            });
    }, [network]);

    if (showObjectState.loadState === 'loaded') {
        return <ValidatorObjectLoaded data={showObjectState as DataType} />;
    }
    if (showObjectState.loadState === 'pending') {
        return (
            <div className={theme.pending}>Please wait for results to load</div>
        );
    }
    if (showObjectState.loadState === 'fail') {
        return <Fail />;
    }

    return <div>"Something went wrong"</div>;
};

const Fail = (): JSX.Element => {
    return (
        <ErrorResult id={''} errorMsg="Validator data could not be loaded" />
    );
};

function instanceOfDataType(object: any): object is DataType {
    return (
        object !== undefined &&
        object !== null &&
        ['status', 'details'].every((x) => x in object)
    );
}

const ValidatorResult = (): JSX.Element => {
    const { state } = useLocation();

    if (instanceOfDataType(state)) {
        return <ValidatorObjectLoaded data={state} />;
    }

    //return IS_STATIC_ENV ? (
    //    <ObjectResultStatic objID={VALIDATOR_OBJECT_ID} />
    //) : (
    return <ValidatorsResultAPI />;
    //);
};



export { ValidatorResult };
