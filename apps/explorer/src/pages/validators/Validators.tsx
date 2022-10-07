// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    Base64DataBuffer,
    type GetObjectDataResponse,
    isSuiMoveObject,
    isSuiObject,
} from '@mysten/sui.js';
import { useState, useContext, useEffect } from 'react';
import { useLocation } from 'react-router-dom';

import ErrorResult from '../../components/error-result/ErrorResult';
import Longtext from '../../components/longtext/Longtext';
import TabFooter from '../../components/tabs/TabFooter';
import { STATE_DEFAULT } from '../../components/top-validators-card/TopValidatorsCard';
import { NetworkContext } from '../../context';
import theme from '../../styles/theme.module.css';
import { DefaultRpcClient as rpc } from '../../utils/api/DefaultRpcClient';
import { IS_STATIC_ENV } from '../../utils/envUtil';
import { truncate } from '../../utils/stringUtils';
import { mockState } from './mockData';

import { Heading } from '~/ui/Heading';
import TableCard from '~/ui/TableCard';

const textDecoder = new TextDecoder();

export type ObjFields = {
    type: string;
    fields: any[keyof string];
};

export type SystemParams = {
    type: '0x2::sui_system::SystemParameters';
    fields: {
        max_validator_candidate_count: number;
        min_validator_stake: bigint;
    };
};

export type Validator = {
    type: '0x2::validator::Validator';
    fields: {
        delegation: bigint;
        delegation_count: number;
        metadata: ValidatorMetadata;
        pending_delegation: bigint;
        pending_delegation_withdraw: bigint;
        pending_delegator_count: number;
        pending_delegator_withdraw_count: number;
        pending_stake: {
            type: '0x1::option::Option<0x2::balance::Balance<0x2::sui::SUI>>';
            fields: any[keyof string];
        };
        pending_withdraw: bigint;
        stake_amount: bigint;
    };
};

export type ValidatorMetadata = {
    type: '0x2::validator::ValidatorMetadata';
    fields: {
        name: string;
        net_address: string;
        next_epoch_stake: number;
        pubkey_bytes: string;
        sui_address: string;
    };
};

export type ValidatorState = {
    delegation_reward: number;
    epoch: number;
    id: { id: string; version: number };
    parameters: SystemParams;
    storage_fund: number;
    treasury_cap: ObjFields;
    validators: {
        type: '0x2::validator_set::ValidatorSet';
        fields: {
            delegation_stake: bigint;
            active_validators: Validator[];
            next_epoch_validators: Validator[];
            pending_removals: string;
            pending_validators: string;
            quorum_stake_threshold: bigint;
            total_validator_stake: bigint;
        };
    };
};

function instanceOfValidatorState(object: any): object is ValidatorState {
    return (
        object !== undefined &&
        object !== null &&
        [
            'validators',
            'epoch',
            'treasury_cap',
            'parameters',
            'delegation_reward',
        ].every((x) => x in object)
    );
}

const VALIDATORS_OBJECT_ID = '0x05';

export function getValidatorState(network: string): Promise<ValidatorState> {
    return rpc(network)
        .getObject(VALIDATORS_OBJECT_ID)
        .then((objState: GetObjectDataResponse) => {
            if (
                isSuiObject(objState.details) &&
                isSuiMoveObject(objState.details.data)
            ) {
                return objState.details.data.fields as ValidatorState;
            }

            throw new Error(
                'sui system state information not shaped as expected'
            );
        });
}

const ValidatorPageResult = (): JSX.Element => {
    const { state } = useLocation();

    if (instanceOfValidatorState(state)) {
        return <ValidatorsPage state={state} />;
    }

    return IS_STATIC_ENV ? (
        <ValidatorsPage state={mockState} />
    ) : (
        <ValidatorPageAPI />
    );
};

export function processValidators(set: Validator[]) {
    return set.map((av, i) => {
        const rawName = av.fields.metadata.fields.name;
        const name = textDecoder.decode(
            new Base64DataBuffer(rawName).getData()
        );
        return {
            name: name,
            address: av.fields.metadata.fields.sui_address,
            pubkeyBytes: av.fields.metadata.fields.pubkey_bytes,
            position: i + 1,
        };
    });
}

export function getTabFooter(count: number) {
    return {
        stats: {
            count: count,
            stats_text: 'total validators',
        },
    };
}

function ValidatorsPage({ state }: { state: ValidatorState }): JSX.Element {
    const validatorsData = processValidators(
        state.validators.fields.active_validators
    );

    const tableData = {
        data: validatorsData.map((validator) => {
            return {
                name: validator.name,
                position: validator.position,
                address: (
                    <Longtext
                        text={validator.address}
                        alttext={truncate(validator.address, 14)}
                        category={'addresses'}
                        isLink={true}
                    />
                ),
                pubkeyBytes: (
                    <Longtext
                        text={validator.pubkeyBytes}
                        alttext={truncate(validator.pubkeyBytes, 14)}
                        category={'addresses'}
                        isLink={false}
                    />
                ),
            };
        }),
        columns: [
            {
                headerLabel: '#',
                accessorKey: 'position',
            },
            {
                headerLabel: 'Name',
                accessorKey: 'name',
            },
            {
                headerLabel: 'Address',
                accessorKey: 'address',
            },
            {
                headerLabel: 'Pubkey Bytes',
                accessorKey: 'pubkeyBytes',
            },
        ],
    };

    return (
        <div>
            <Heading as="h1" variant="heading2" weight="bold">
                Validators
            </Heading>
            <div className="mt-8">
                <TableCard tabledata={tableData} />
                <TabFooter stats={getTabFooter(validatorsData.length).stats}>
                    <Longtext
                        text=""
                        category="validators"
                        isLink={false}
                        alttext=""
                    />
                </TabFooter>
            </div>
        </div>
    );
}

export const ValidatorLoadFail = (): JSX.Element => {
    return (
        <ErrorResult id={''} errorMsg="Validator data could not be loaded" />
    );
};

export const ValidatorPageAPI = (): JSX.Element => {
    const [showObjectState, setObjectState] = useState(STATE_DEFAULT);
    const [loadState, setLoadState] = useState('pending');
    const [network] = useContext(NetworkContext);
    useEffect(() => {
        getValidatorState(network)
            .then((objState: ValidatorState) => {
                setObjectState(objState);
                setLoadState('loaded');
            })
            .catch((error: any) => {
                console.error(error);
                setLoadState('fail');
            });
    }, [network]);

    if (loadState === 'loaded') {
        return <ValidatorsPage state={showObjectState as ValidatorState} />;
    }
    if (loadState === 'pending') {
        return <div className={theme.pending}>loading validator info...</div>;
    }
    if (loadState === 'fail') {
        return <ValidatorLoadFail />;
    }

    return <div>"Something went wrong"</div>;
};

export { ValidatorPageResult };
