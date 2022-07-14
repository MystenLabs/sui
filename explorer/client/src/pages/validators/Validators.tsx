import { Base64DataBuffer } from '@mysten/sui.js';
import { useState, useContext, useEffect } from 'react';
import { useLocation } from 'react-router-dom';

import ErrorResult from '../../components/error-result/ErrorResult';
import Longtext from '../../components/longtext/Longtext';
import TableCard from '../../components/table/TableCard';
import TabFooter from '../../components/tabs/TabFooter';
import Tabs from '../../components/tabs/Tabs';
import {
    getValidatorState,
    sortValidatorsByStake,
    STATE_DEFAULT,
    type ValidatorState,
} from '../../components/top-validators-card/TopValidatorsCard';
import styles from '../../components/top-validators-card/TopValidatorsCard.module.css';
import { NetworkContext } from '../../context';
import theme from '../../styles/theme.module.css';
import { IS_STATIC_ENV } from '../../utils/envUtil';
import { truncate } from '../../utils/stringUtils';
import { mockState } from './mockData';

const textDecoder = new TextDecoder();

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

export function stakeColumn(validator: {
    stake: BigInt;
    stakePercent: number;
}): JSX.Element {
    return (
        <div>
            {' '}
            {validator.stake.toString()}{' '}
            <span className={styles.stakepercent}>
                {' '}
                {validator.stakePercent.toFixed(2)} %
            </span>
        </div>
    );
}

export const getStakePercent = (stake: bigint, total: bigint): number =>
    Number(BigInt(stake) * BigInt(100)) / Number(total);

function ValidatorsPage({ state }: { state: ValidatorState }): JSX.Element {
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
                address: av.fields.metadata.fields.sui_address,
                stake: av.fields.stake_amount,
                stakePercent: getStakePercent(
                    av.fields.stake_amount,
                    totalStake
                ),
                delegation_count: av.fields.delegation_count || 0,
                position: i + 1,
            };
        }
    );

    let cumulativeStakePercent = 0;
    // map the above data to match the table combine stake and stake percent
    const tableData = {
        data: validatorsData.map((validator) => {
            cumulativeStakePercent += validator.stakePercent;
            return {
                name: validator.name,
                address: (
                    <Longtext
                        text={validator.address}
                        alttext={truncate(validator.address, 14)}
                        category={'addresses'}
                        isLink={true}
                    />
                ),
                stake: stakeColumn(validator),
                cumulativeStake: (
                    <span className={styles.stakepercent}>
                        {' '}
                        {cumulativeStakePercent.toFixed(2)} %
                    </span>
                ),
                delegation: validator.delegation_count,
                position: validator.position,
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
                headerLabel: 'STAKE',
                accessorKey: 'stake',
            },
            {
                headerLabel: 'Cumulative Stake',
                accessorKey: 'cumulativeStake',
            },
            {
                headerLabel: 'Delegators',
                accessorKey: 'delegation',
            },
            {
                headerLabel: 'Address',
                accessorKey: 'address',
            },
        ],
    };

    const tabsFooter = {
        stats: {
            count: validatorsData.length,
            stats_text: 'total validators',
        },
    };

    return (
        <div className={styles.validators}>
            <Tabs selected={0}>
                <div title="Validators">
                    <TableCard tabledata={tableData} />
                    <TabFooter stats={tabsFooter.stats}>
                        <Longtext
                            text=""
                            category="validators"
                            isLink={false}
                            isCopyButton={false}
                            /*showIconButton={true}*/
                            alttext=""
                        />
                    </TabFooter>
                </div>
                <div title=""></div>
            </Tabs>
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
