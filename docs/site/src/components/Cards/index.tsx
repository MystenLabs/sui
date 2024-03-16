// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useEffect, useState } from "react";
import { useLocation, useHistory } from "@docusaurus/router";
import { useDocsVersion } from "@docusaurus/theme-common/internal";
import Markdown from "markdown-to-jsx";
import styles from "./styles.module.css";

export function Card(props) {
  const location = useLocation();
  const history = useHistory();
  const isHome = location.pathname === "/";
  const [href, setHref] = useState();

  useEffect(() => {
    if (href) {
      window.open(href, "_blank");
    }
    return true;
  }, [href]);

  const docs = useDocsVersion().docs;
  let h = props.href;
  if (h.match(/^\//)) {
    h = h.substring(1);
  }
  let i = Object.entries(docs).find((doc) => doc[0] === h);
  let description = "";
  if (Array.isArray(i)) {
    description = i[1].description;
  }

  const handleClick = (loc) => {
    if (loc.match(/^https?/)) {
      setHref(loc);
    } else {
      history.push(loc);
    }
  };

  return (
    <div className={styles.card} onClick={() => handleClick(props.href)}>
      <div className={styles.card__header}>
        <h2 className={styles.card__header__copy}>{props.title}</h2>
      </div>

      <div className={styles.card__copy}>
        <Markdown>{props.children ? props.children : description}</Markdown>
      </div>
    </div>
  );
}

export function Cards({ children, ...props }) {
  const location = useLocation();
  let twClassList =
    location.pathname === "/"
      ? `gap-16 grid lg:grid-rows-${Math.ceil(
          children.length / 3,
        )} md:grid-rows-${Math.ceil(
          children.length / 2,
        )} lg:grid-cols-3 md:grid-cols-2`
      : "grid-card gap-8 grid xl:grid-rows-${Math.ceil(children.length/3)} lg:grid-rows-${Math.ceil(children.length/2)} xl:grid-cols-3 lg:grid-cols-2 justify-start pb-8";
  return (
    <div className={twClassList} {...props}>
      {children}
    </div>
  );
}
