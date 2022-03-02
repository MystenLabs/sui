import { memo } from 'react';

import TruncatedLabel from '../../truncated-label/TruncatedLabel';

type AccountIDProps = {
    id: string;
};

function AccountID({ id }: AccountIDProps) {
    // TODO: link to details page when available
    return <TruncatedLabel label={id} />;
}

export default memo(AccountID);
