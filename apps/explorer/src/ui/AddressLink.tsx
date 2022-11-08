// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Link } from '~/ui/Link';

export interface AddressLinkProps {
    text?: string;
    link: string;
}

//TODO: We might want extend by adding a category prop to this component to account for txn and object.
export function AddressLink({ text, link }: AddressLinkProps) {
    return (
        <Link variant="mono" to={`/addresses/${encodeURIComponent(link)}`}>
            {text ?? link}
        </Link>
    );
}
