// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import TopBar from "./TopBar";

const Header = () => {
    return (
        <>
            <TopBar />
            <div className="bg-sui-ocean-dark">
                This is a header
            </div>
        </>
    )
}

export default Header;