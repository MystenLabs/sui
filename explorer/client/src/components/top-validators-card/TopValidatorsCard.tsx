// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useContext, useEffect, useState } from 'react';

import Longtext from '../../components/longtext/Longtext';
import TableCard from '../../components/table/TableCard';
import Tabs from '../../components/tabs/Tabs';
import { NetworkContext } from '../../context';
import {
    getValidatorState,
    processValidators,
    ValidatorLoadFail,
    type ValidatorState,
} from '../../pages/validators/Validators';
import { mockState } from '../../pages/validators/mockData';
import theme from '../../styles/theme.module.css';
import { truncate } from '../../utils/stringUtils';

import styles from './TopValidatorsCard.module.css';

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
            total_validator_stake: BigInt(0),
        },
    },
};

export const TopValidatorsCardStatic = (): JSX.Element => {
    return <TopValidatorsCard state={mockState as ValidatorState} />;
};

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

function TopValidatorsCard({ state }: { state: ValidatorState }): JSX.Element {
    const validatorsData = processValidators(
        state.validators.fields.active_validators
    );

    // map the above data to match the table - combine stake and stake percent
    const tableData = {
        data: validatorsData.map((validator) => ({
            name: validator.name,
            position: validator.position,
            address: (
                <Longtext
                    text={validator.address}
                    alttext={truncate(validator.address, 14)}
                    category={'addresses'}
                    isLink={true}
                    isCopyButton={false}
                />
            ),
            pubkeyBytes: (
                <Longtext
                    text={validator.pubkeyBytes}
                    alttext={truncate(validator.pubkeyBytes, 14)}
                    category={'addresses'}
                    isLink={false}
                    isCopyButton={false}
                />
            ),
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
        <div className={styles.validators}>
            <Tabs selected={0}>
                <div title="Top Validators">
                    <TableCard tabledata={tableData} />
                    {/* <TabFooter stats={tabsFooter.stats}>
                        <Longtext
                            text=""
                            category="validators"
                            isLink={true}
                            isCopyButton={false}
                            alttext="More Validators"
                        />
                    </TabFooter> */}
                </div>
            </Tabs>
        </div>
    );
}

export default TopValidatorsCard;
