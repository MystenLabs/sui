// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    type GetObjectDataResponse,
    isSuiMoveObject,
    isSuiObject,
} from '@mysten/sui.js';
import { useContext, useEffect, useState } from 'react';

import Longtext from '../../components/longtext/Longtext';
import TableCard from '../../components/table/TableCard';
import TabFooter from '../../components/tabs/TabFooter';
import Tabs from '../../components/tabs/Tabs';
import { NetworkContext } from '../../context';
import theme from '../../styles/theme.module.css';
import { DefaultRpcClient as rpc } from '../../utils/api/DefaultRpcClient';
import ErrorResult from '../error-result/ErrorResult';

import styles from './TopValidatorsCard.module.css';

type ObjFields = {
    type: string;
    fields: any[keyof string];
};

type SystemParams = {
    type: '0x2::sui_system::SystemParameters';
    fields: {
        max_validator_candidate_count: number;
        min_validator_stake: bigint;
    };
};

type Validator = {
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
        stake: bigint;
    };
};

type ValidatorMetadata = {
    type: '0x2::validator::ValidatorMetadata';
    fields: {
        name: string;
        net_address: string;
        next_epoch_stake: number;
        pubkey_bytes: string;
        sui_address: string;
    };
};

type ValidatorState = {
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
            validator_stake: bigint;
        };
    };
};

const STATE_DEFAULT = {
    delegation_reward: 0,
    epoch: 0,
    id: { id: '', version: 0 },
    parameters: {},
    storage_fund: 0,
    treasury_cap: {
        fields: {},
    },
    validators: {
        type: '0x2::validator_set::ValidatorSet',
        fields: {
            delegation_stake: BigInt(0),
            active_validators: [],
            next_epoch_validators: [],
            pending_removals: '',
            pending_validators: '',
            quorum_stake_threshold: BigInt(0),
            validator_stake: BigInt(0),
        },
    },
};

const VALIDATORS_OBJECT_ID = '0x05';

function getValidatorState(network: string): Promise<ValidatorState> {
    return rpc(network)
        .getObject(VALIDATORS_OBJECT_ID)
        .then((objState: GetObjectDataResponse) => {
            //console.log(objState);
            if (
                isSuiObject(objState.details) &&
                isSuiMoveObject(objState.details.data)
            ) {
                console.log(objState.details.data.fields);
                return objState.details.data.fields as ValidatorState;
            }

            throw new Error(
                'sui system state information not shaped as expected'
            );
        });
}

const Fail = (): JSX.Element => {
    return (
        <ErrorResult id={''} errorMsg="Validator data could not be loaded" />
    );
};

export const TopValidatorsCardAPI = (): JSX.Element => {
    const [showObjectState, setObjectState] = useState({});
    const [loadState, setLoadState] = useState('pending');
    const [network] = useContext(NetworkContext);
    useEffect(() => {
        getValidatorState(network)
            .then((objState: ValidatorState) => {
                console.log('validator state', objState);
                setObjectState(objState);
                setLoadState('loaded');
            })
            .catch((error: any) => {
                console.log(error);
                setObjectState(STATE_DEFAULT);
                setLoadState('fail');
            });
    }, [network]);

    if (loadState === 'loaded') {
        console.log('VALIDATORS LOADED');
        return <TopValidatorsCard state={showObjectState as ValidatorState} />;
    }
    if (loadState === 'pending') {
        return <div className={theme.pending}>loading validator info...</div>;
    }
    if (loadState === 'fail') {
        return <Fail />;
    }

    return <div>"Something went wrong"</div>;
};

/*
const validatorsDataOld = [
    {
        validatorName: 'Jump Crypto',
        suiStake: 9_220_000,
        suiStakePercent: '5.2%',
        eporchReward: '38,026',
        position: 1,
    },
    {
        validatorName: 'Blockdaemon',
        suiStake: 8_220_000,
        suiStakePercent: '4.2%',
        eporchReward: '34,100',
        position: 2,
    },
    {
        validatorName: 'Kraken',
        suiStake: 4_650_000,
        suiStakePercent: '2.69%',
        eporchReward: '19,220',
        position: 3,
    },
    {
        validatorName: 'Coinbase',
        suiStake: 4_550_000,
        suiStakePercent: '2.63%',
        eporchReward: '18,806',
        position: 4,
    },
    {
        validatorName: 'a16z',
        suiStake: 2_860_000,
        suiStakePercent: '1.58%',
        eporchReward: '11,821',
        position: 5,
    },
    {
        validatorName: 'Figment',
        suiStake: 2_840_000,
        suiStakePercent: '1.63%',
        eporchReward: '11,736',
        position: 6,
    },
    {
        validatorName: '0x813e...d21f07',
        suiStake: 2_730_000,
        suiStakePercent: '1.58%',
        eporchReward: '11,736',
        position: 7,
    },
    {
        validatorName: '0x813e...d21f07',
        suiStake: 2_730_000,
        suiStakePercent: '1.58%',
        eporchReward: '11,736',
        position: 8,
    },
    {
        validatorName: '0x813e...d21f07',
        suiStake: 2_730_000,
        suiStakePercent: '1.58%',
        eporchReward: '11,736',
        position: 9,
    },
    {
        validatorName: '0x813e...d21f07',
        suiStake: 2_730_000,
        suiStakePercent: '1.58%',
        eporchReward: '11,736',
        position: 10,
    },
];
*/

// TODO: Specify the type of the context
// Specify the type of the context
function TopValidatorsCard({ state }: { state: ValidatorState }) {
    // mock validators data
    const totalStake = state.validators.fields.validator_stake;
    const validatorsData = state.validators.fields.active_validators.map(
        (av, i) => {
            return {
                name: av.fields.metadata.fields.name,
                stake: av.fields.stake,
                stakePercent: av.fields.stake / totalStake,
                position: i + 1,
            };
        }
    );

    // map the above data to match the table combine stake and stake percent
    const mockValidatorsData = {
        data: validatorsData.map((validator) => ({
            validatorName: validator.name,
            stake: (
                <div>
                    {' '}
                    {validator.stake}{' '}
                    <span className={styles.stakepercent}>
                        {' '}
                        {validator.stakePercent}
                    </span>
                </div>
            ),
            position: validator.position,
        })),
        columns: [
            {
                headerLabel: '#',
                accessorKey: 'position',
            },
            {
                headerLabel: 'Validator',
                accessorKey: 'name',
            },
            {
                headerLabel: 'STAKE',
                accessorKey: 'stake',
            },
        ],
    };

    const tabsFooter = {
        stats: {
            count: 15482,
            stats_text: 'total transactions',
        },
    };

    return (
        <div className={styles.validators}>
            <Tabs selected={0}>
                <div title="Top Validators">
                    <TableCard tabledata={mockValidatorsData} />
                    <TabFooter stats={tabsFooter.stats}>
                        <Longtext
                            text=""
                            category="transactions"
                            isLink={true}
                            isCopyButton={false}
                            showIconButton={true}
                            alttext="More Validators"
                        />
                    </TabFooter>
                </div>
                <div title=""></div>
            </Tabs>
        </div>
    );
}

export default TopValidatorsCard;
