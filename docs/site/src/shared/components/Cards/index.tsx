/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

import React, { useEffect, useState } from "react";
import { useHistory } from "@docusaurus/router";
import { usePluginData } from "@docusaurus/useGlobalData";
import styles from "./styles.module.css";

interface CardProps {
  title: string;
  href: string;
  className?: string;
  children?: React.ReactNode;
}

export function Card({ title, href, className, children }: CardProps) {
  const history = useHistory();
  const [url, setUrl] = useState();

  useEffect(() => {
    if (url) {
      window.open(url, "_blank");
    }
    return;
  }, [url]);

  const { descriptions } = usePluginData("sui-description-plugin");
  let h = href;
  if (!h.match(/^\//)) {
    h = `/${h}`;
  }
  const d = descriptions.find((desc) => desc["id"] === h);
  let description = "";
  if (typeof d !== "undefined") {
    description = d.description;
  }

  const handleClick = (loc) => {
    if (loc.match(/^https?/)) {
      setUrl(loc);
    } else {
      history.push(loc);
    }
  };

  return (
    <div
      className={`${styles.card} ${className}`}
      onClick={() => handleClick(href)}
    >
      <div className={styles.card__header}>
        <h2 className={styles.card__header__copy}>{title}</h2>
      </div>

      <div className={styles.card__copy}>
        {children ? children : description}
      </div>
    </div>
  );
}

interface CardsProps {
  children: React.ReactNode;
  type?: string;
  [key: string]: any;
}

export function Cards({ children, type, ...props }: CardsProps) {
  const baseClasses = [
    "grid-card",
    "gap-8",
    "grid",
    "xl:grid-cols-3",
    "lg:grid-cols-2",
    "justify-start",
    "pb-8",
  ].join(" ");

  const typeClass = type === "steps"
    ? styles["step-card-container"]
    : styles["card-container"];

  return (
    <div className={`${baseClasses} ${typeClass}`} {...props}>
      {children}
    </div>
  );
}
