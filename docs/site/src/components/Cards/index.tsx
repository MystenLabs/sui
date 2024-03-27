// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useEffect, useState, useContext } from "react";
import { useHistory } from "@docusaurus/router";
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import {usePluginData} from '@docusaurus/useGlobalData';
import styles from "./styles.module.css";

export function Card(props) {
  const history = useHistory();
  const [href, setHref] = useState();

  useEffect(() => {
    if (href) {
      window.open(href, "_blank");
    }
    return;
  }, [href]);

  const { descriptions } = usePluginData("sui-description-plugin");
  let h = props.href;
  if (h.match(/^\//)) {
    h = h.substring(1);
  }
  const d = descriptions.find((desc) => desc["id"] === h);
  let description = "";
  if (typeof d !== "undefined") {
    description = d.description;
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
        {props.children ? props.children : description}
      </div>
    </div>
  );
}

export function Cards({ children, ...props }) {
  let twClassList = "grid-card gap-8 grid xl:grid-rows-${Math.ceil(children.length/3)} lg:grid-rows-${Math.ceil(children.length/2)} xl:grid-cols-3 lg:grid-cols-2 justify-start pb-8";
  return (
    <div className={twClassList} {...props}>
      {children}
    </div>
  );
}
