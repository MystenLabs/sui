// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import LoadingIndicator from '_src/ui/app/components/loading/LoadingIndicator';

interface Props {
    url: string;
}

export function OnrampIframe({ url }: Props) {
    return (
        <div>
            {/* This loading indicator appears under the iframe, so that while the iframe loads the user gets some feedback */}
            <div className="h-full w-full flex items-center justify-center z-0">
                <LoadingIndicator />
            </div>

            <iframe
                title="Buy"
                src={url}
                className="w-full h-full inset-0 absolute outline-none border-none z-10"
                allow="camera;microphone;payment"
            />
        </div>
    );
}
