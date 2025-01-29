// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import React, { useState, useEffect } from "react";

export default function Protocol(props) {
  const { toc } = props;
  const [proto, setProto] = useState(toc[0]);
  const [messages, setMessages] = useState(toc[0].messages);
  const [moveRight, setMoveRight] = useState(false);
  const triggerY = 140; // Y-position where movement happens

  useEffect(() => {
    const handleScroll = () => {
      if (window.scrollY >= triggerY) {
        setMoveRight(true);
      } else {
        setMoveRight(false);
      }
    };

    window.addEventListener("scroll", handleScroll);
    return () => window.removeEventListener("scroll", handleScroll);
  }, []);

  if (!toc) {
    return;
  }

  const handleProtoChange = (e) => {
    const selected = e.target.value;
    const selectedProto = toc[selected]; // Get the selected protocol
    setProto(selectedProto);
    setMessages(selectedProto.messages);
  };
  const handleMessageChange = (e) => {
    const selected = e.target.value;
    const message = proto.messages.filter((item) => {
      return item.name === selected;
    });
    const hash = message[0].link;
    window.location.hash = `#${hash}`;
  };

  return (
    <div
      className={`max-xl:hidden sticky top-24 left-0 transition delay-150 ease-in-out] ${moveRight ? "translate-x-1/2" : ""}`}
    >
      <select className="p-2 w-[200px]" onChange={handleProtoChange}>
        {toc.map((item, i) => {
          return (
            <option key={i} value={i}>
              {item.name}
            </option>
          );
        })}
      </select>
      <select className="p-2 w-[200px]" onChange={handleMessageChange}>
        {messages.map((message) => {
          return (
            <option key={message.name} value={message.name}>
              {message.name}
            </option>
          );
        })}
      </select>
    </div>
  );
}
