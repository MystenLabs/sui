// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Header from "./Header";
import Modal from "./Modal";
import { ConnectButton } from "@mysten/wallet-kit";
import { io } from "socket.io-client";

const Main = () => {
    const socket = io("http://localhost:8000");
    const join = (e) => {
        socket.emit("join", "New user joined")
    };
    return (
        <>
            <Header />
            <Modal />
            <ConnectButton />
            <div>
                <button onClick={join}>JOIN!</button>
            </div>
        </>
    )
}

export default Main;