// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    type GetObjectDataResponse,
    isSuiMoveObject,
    isSuiObject,
    Base64DataBuffer,
} from '@mysten/sui.js';
import { useContext, useEffect, useState } from 'react';

import Longtext from '../../components/longtext/Longtext';
import TableCard from '../../components/table/TableCard';
import TabFooter from '../../components/tabs/TabFooter';
import Tabs from '../../components/tabs/Tabs';
import { NetworkContext } from '../../context';
import { ValidatorLoadFail } from '../../pages/validators/Validators';
import theme from '../../styles/theme.module.css';
import { DefaultRpcClient as rpc } from '../../utils/api/DefaultRpcClient';

import styles from './TopValidatorsCard.module.css';

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
            validator_stake: bigint;
        };
    };
};

export const STATE_DEFAULT: ValidatorState = {
    delegation_reward: 0,
    epoch: 0,
    id: { id: '', version: 0 },
    parameters: {
        type: '0x2::sui_system::SystemParameters',
        fields: {
            max_validator_candidate_count: 0,
            min_validator_stake: BigInt(0),
        },
    },
    storage_fund: 0,
    treasury_cap: {
        type: '',
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

export function getValidatorState(network: string): Promise<ValidatorState> {
    return rpc(network)
        .getObject(VALIDATORS_OBJECT_ID)
        .then((objState: GetObjectDataResponse) => {
            //console.log(objState);
            if (
                isSuiObject(objState.details) &&
                isSuiMoveObject(objState.details.data)
            ) {
                console.log(objState);
                return objState.details.data.fields as ValidatorState;
            }

            throw new Error(
                'sui system state information not shaped as expected'
            );
        });
}

export const TopValidatorsCardAPI = (): JSX.Element => {
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
                console.log(error);
                setLoadState('fail');
            });
    }, [network]);

    if (loadState === 'loaded') {
        return <TopValidatorsCard state={showObjectState as ValidatorState} />;
    }
    if (loadState === 'pending') {
        return <div className={theme.pending}>loading validator info...</div>;
    }
    if (loadState === 'fail') {
        return <ValidatorLoadFail />;
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

const textDecoder = new TextDecoder('utf-8');

export function sortValidatorsByStake(validators: Validator[]) {
    validators.sort((a: Validator, b: Validator): number => {
        if (a.fields.stake_amount < b.fields.stake_amount) return -1;
        if (a.fields.stake_amount > b.fields.stake_amount) return 1;
        return 0;
    });
}

function TopValidatorsCard({ state }: { state: ValidatorState }): JSX.Element {
    const totalStake = state.validators.fields.validator_stake;
    // sort by order of descending stake
    sortValidatorsByStake(state.validators.fields.active_validators);

    const validatorsData = state.validators.fields.active_validators.map(
        (av, i) => {
            const rawName = av.fields.metadata.fields.name;
            const name = textDecoder.decode(
                new Base64DataBuffer(rawName).getData()
            );
            return {
                name: name,
                stake: av.fields.stake_amount,
                stakePercent: Number(av.fields.stake_amount / totalStake) * 100,
                delegation_count: av.fields.delegation_count || 0,
                position: i + 1,
            };
        }
    );

    // map the above data to match the table combine stake and stake percent
    const tableData = {
        data: validatorsData.map((validator) => ({
            name: validator.name,
            stake: (
                <div>
                    {' '}
                    {validator.stake}{' '}
                    <span className={styles.stakepercent}>
                        {' '}
                        {validator.stakePercent} %
                    </span>
                </div>
            ),
            delegation: validator.delegation_count,
            position: validator.position,
        })),
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
                headerLabel: 'STAKE',
                accessorKey: 'stake',
            },
            {
                headerLabel: 'Delegators',
                accessorKey: 'delegation',
            },
        ],
    };

    const tabsFooter = {
        stats: {
            count: validatorsData.length,
            stats_text: 'total validators',
        },
    };

    console.log(tableData);

    return (
        <div className={styles.validators}>
            <Tabs selected={0}>
                <div title="Top Validators">
                    <TableCard tabledata={tableData} />
                    <TabFooter stats={tabsFooter.stats}>
                        <Longtext
                            text=""
                            category="validators"
                            isLink={true}
                            isCopyButton={false}
                            /*showIconButton={true}*/
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
