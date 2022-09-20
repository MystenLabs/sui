import Link from 'next/link';
import { useMemo } from 'react';
import { useSuiObjects } from '../shared/objects-store-context';
import { STATUS_TO_TXT } from './constants';

export function LotteryListItem({ id }) {
    const { suiObjects } = useSuiObjects();
    const lottery = useMemo(() => suiObjects?.[id] || null, [suiObjects, id]);
    if (!lottery) {
        return null;
    }
    const { capys, round, status } = lottery.data.fields;
    return (
        <>
            <dt>
                <b>
                    <Link href={`/lotteries/${id}`}>{`#${id}`}</Link>
                </b>
            </dt>
            <dd>Round: {round}</dd>
            <dd>Capys: {capys?.length || 0}</dd>
            <dd>Status: {STATUS_TO_TXT[status]}</dd>
        </>
    );
}
