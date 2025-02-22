// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useState, useEffect } from "react";

export default function Protocol(props) {
  const { toc } = props;
  const [proto, setProto] = useState(toc[0]);
  //const [messages, setMessages] = useState(toc[0].messages);
  //const [services, setServices] = useState(toc[0].services);
  //const [enums, setEnums] = useState(toc[0].enums);
  const [belowFold, setBelowFold] = useState(false);
  const triggerY = 140;

  useEffect(() => {
    const handleScroll = () => {
      if (window.scrollY >= triggerY) {
        setBelowFold(true);
      } else {
        setBelowFold(false);
      }
    };

    window.addEventListener("scroll", handleScroll);
    return () => window.removeEventListener("scroll", handleScroll);
  }, []);

  if (!toc) {
    return;
  }

  const handleProtoChange = (e) => {
    console.log(toc);
    const selected = e.target.value;
    const selectedProto = toc[selected]; // Get the selected protocol
    setProto(selectedProto);
    //setMessages(selectedProto.messages);
    //setServices(selectedProto.services);
    //setEnums(selectedProto.enums);
    window.location.hash = `#${selectedProto.link}`;
  };
  const handleMessageChange = (e) => {
    const selected = e.target.value;
    const message = proto.messages.filter((item) => {
      return item.name === selected;
    });
    const hash = message[0].link;
    window.location.hash = `#${hash}`;
  };
  const handleServicesChange = (e) => {
    const selected = e.target.value;
    const service = proto.services.filter((item) => {
      return item.name === selected;
    });
    const hash = service[0].link;
    window.location.hash = `#${hash}`;
  };
  const handleEnumsChange = (e) => {
    const selected = e.target.value;
    const num = proto.enums.filter((item) => {
      return item.name === selected;
    });
    const hash = num[0].link;
    window.location.hash = `#${hash}`;
  };

  return (
    <div
      className={`max-xl:hidden sticky top-16 py-4 -mx-4 z-10 backdrop-blur-sm border-sui-ghost-white dark:border-sui-ghost-dark ${belowFold ? "border-solid border-x-0 border-t-0 border-b" : ""}`}
    >
      <style>
        {`
          h2, h3 {
            scroll-margin:126px !important;
          }
        `}
      </style>
      <label
        className="m-2 text-xs bg-sui-white rounded-lg backdrop-blur-none"
        htmlFor="proto"
      >
        Proto files
      </label>
      <select id="proto" className="p-2 w-[200px]" onChange={handleProtoChange}>
        {toc.map((item, i) => {
          return (
            <option key={i} value={i}>
              {item.name}
            </option>
          );
        })}
      </select>
      {proto.messages.length > 0 && (
        <>
          <label className="mx-2 text-xs" htmlFor="messages">
            Messages
          </label>
          <select
            id="messages"
            className="p-2 w-[200px]"
            onChange={handleMessageChange}
          >
            {proto.messages.map((message) => {
              return (
                <option key={message.name} value={message.name}>
                  {message.name}
                </option>
              );
            })}
          </select>
        </>
      )}
      {proto.services.length > 0 && (
        <>
          <label className="mx-2 text-xs" htmlFor="services">
            Services
          </label>
          <select
            id="services"
            className="p-2 w-[200px]"
            onChange={handleServicesChange}
          >
            {proto.services.map((service) => {
              return (
                <option key={service.name} value={service.name}>
                  {service.name}
                </option>
              );
            })}
          </select>
        </>
      )}
      {proto.enums.length > 0 && (
        <>
          <label className="mx-2 text-xs" htmlFor="enums">
            Enums
          </label>
          <select
            id="enums"
            className="p-2 w-[200px]"
            onChange={handleEnumsChange}
          >
            {proto.enums.map((num) => {
              return (
                <option key={num.name} value={num.name}>
                  {num.name}
                </option>
              );
            })}
          </select>
        </>
      )}
    </div>
  );
}
