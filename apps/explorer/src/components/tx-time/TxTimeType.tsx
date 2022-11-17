// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useTimeAgo } from '../../utils/timeUtils';

type Prop = {
    timestamp: number | undefined;
};

export function TxTimeType({ timestamp }: Prop) {
    const timeAgo = useTimeAgo(timestamp, true);

    return (
        <section>
            <div className="text-caption w-20">{timeAgo}</div>
        </section>
    );
}
