import { Base64DataBuffer } from "@mysten/sui.js";
import cl from 'classnames';
import { useState, useContext, useEffect } from "react";
import { useLocation } from "react-router-dom";

import ErrorResult from "../../components/error-result/ErrorResult";
import { NetworkContext } from "../../context";
import theme from '../../styles/theme.module.css';
import { type DataType, getObjectDataWithPackageAddress } from "../object-result/ObjectResult";

import objStyles from '../object-result/ObjectResult.module.css';
import txStyles from '../transaction-result/TransactionResult.module.css';


const VALIDATORS_OBJECT_ID = '0x05';

const DATATYPE_DEFAULT = { loadState: 'pending' };


type ObjFields = {
    type: string,
    fields: any[keyof string]
}

type ValidatorState = {
    delegation_reward: number,
    epoch: number,
    id: { id: string, version: number },
    parameters: ObjFields,
    storage_fund: number,
    treasury_cap: ObjFields,
    validators: {
        type: '0x2::validator_set::ValidatorSet'
        fields: {
            delegation_stake: number,
            active_validators: ObjFields[],
            next_epoch_validators: ObjFields[],
            pending_removals: string,
            pending_validators: string,
            quorum_stake_threshold: number,
            validator_stake: number
        },
    }
}

const textDecoder = new TextDecoder();

function ValidatorObjectLoaded({ data }: { data: DataType }): JSX.Element {
    console.log('validator object loaded', data);

    const contents = data.data['contents'] as ValidatorState;
    console.log(contents);

    let active_set = contents.validators.fields.active_validators;

    return (
        <>
            <div id="validators">
                <h1 className={objStyles.clickableheader}>
                    Validators
                </h1>

                <div id="activeset">
                    <h2>Active</h2>

                    {active_set.map((itm: any, i: number) => (
                        <div
                        key={i}
                        className={cl(
                            txStyles.txcardgrid,
                            itm.className
                                ? txStyles[itm.className]
                                : ''
                        )}>

                            <div>
                            <h3>{textDecoder.decode(new Base64DataBuffer(itm.fields['metadata'].fields.name).getData())}
                            </h3>

                            <div>
                            <div>
                                <h4>Address</h4>
                                {itm.fields['metadata'].fields.sui_address}
                            </div>
                            <div>
                                <h4>Stake</h4>
                                {itm.fields['stake']}
                            </div>
                            </div>

                            <div>
                                <div>
                                    <h5>Delegation</h5>
                                    {itm.fields['delegation']}
                                </div>
                                <div>
                                    <h5>Delegator Count</h5>
                                    {itm.fields['delegator_count']}
                                </div>
                            </div>
                            <div>
                                <div>
                                    <h5>Pending Delegation</h5>
                                    {itm.fields['pending_delegation']}
                                </div>
                                <div>
                                    <h5>Pending Delegation Withdraw</h5>
                                    {itm.fields['pending_delegation_withdraw']}
                                </div>
                            </div>
                            <div>
                                <div>
                                    <h5>Pending Delegators</h5>
                                    {itm.fields['pending_delegator_count']}
                                </div>
                                <div>
                                    <h5>Pending Delegator Withdraws</h5>
                                    {itm.fields['pending_delegator_withdraw_count']}
                                </div>
                            </div>
                            </div>

                            <br/>
                        </div>
                    ))}
                </div>
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
        return <Fail/>;
    }

    return <div>"Something went wrong"</div>;
};

const Fail = (): JSX.Element => {
    return (
        <ErrorResult
            id={""}
            errorMsg="Validator data could not be loaded"
        />
    );
};

function instanceOfDataType(object: any): object is DataType {
    return object !== undefined && object !== null && ['status', 'details'].every((x) => x in object);
}

const ValidatorResult = (): JSX.Element => {
    const { state } = useLocation();

    if (instanceOfDataType(state)) {
        return <ValidatorObjectLoaded data={state} />;
    }

    //return IS_STATIC_ENV ? (
    //    <ObjectResultStatic objID={VALIDATOR_OBJECT_ID} />
    //) : (
        return <ValidatorsResultAPI/>
    //);
};


export { ValidatorResult }