import { memo } from 'react';

import TruncatedLabel from '../../truncated-label/TruncatedLabel';

type ObjectIDProps = {
    id: string;
    size?: 'small' | 'normal';
};

function ObjectID({ id, size = 'normal' }: ObjectIDProps) {
    // TODO: link to details page when available
    return <TruncatedLabel label={id} size={size} />;
}

export default memo(ObjectID);
