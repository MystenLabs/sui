import { useState } from 'react';
import { useMemo } from 'react';
import { useSuiObjects } from '../shared/objects-store-context';
import { STATUS_ENDED, TYPE_LOTTERY } from './constants';
import { LotteryListItem } from './list-item';

export function LotteryList() {
    const [onlyLive, setOnlyLive] = useState(true);
    const { suiObjects } = useSuiObjects();
    const lotteries = useMemo(() => {
        return Object.entries(suiObjects)
            .filter(
                ([_, obj]) =>
                    obj.data.type === TYPE_LOTTERY &&
                    (!onlyLive || obj.data.fields.status !== STATUS_ENDED)
            )
            .map(([_, obj]) => obj);
    }, [suiObjects, onlyLive]);

    return (
        <>
            <label style={{ marginTop: '10px' }}>
                <input
                    type="checkbox"
                    checked={onlyLive}
                    onChange={(e) => setOnlyLive(e.currentTarget.checked)}
                />{' '}
                <span class="checkable">Live</span>
            </label>
            <dl>
                {lotteries.map(({ reference: { objectId: id } }) => (
                    <LotteryListItem key={id} id={id} />
                ))}
            </dl>
        </>
    );
}
