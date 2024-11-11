// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import doc from '@doc-data';
import Markdoc from '@markdoc/markdoc';
import { heading } from '../schema/Heading.markdoc';

/** @type {import('@markdoc/markdoc').Config} */
const config = {
  nodes: {
    heading
  }
};

interface Heading {
  id: string;
  title: string;
}

function collectHeadings(node: any, sections = []) {
  if (node && node.name === "article") {
    // Match all h1, h2, h3… tags
    if (node.children.length > 0){
      node.children.map((n) => {
        if (n.name.match(/h(?!1)\d/)){
          const title = n.children[0];
          const id = createId(title);

          sections.push({...node.attributes, title, id})
        }
      })
    }
  }

  return sections;
}

function createId(title: string) {

  const id = title.toLowerCase().replace(/ /g, "-");
  return id;

}

export default function Help() {

  const ast = Markdoc.parse(doc);
  const content = Markdoc.transform(ast, config);
  const nav: Heading[] = collectHeadings(content);
  
  return (
    <div className="flex gap-4">
      <nav className="flex-none w-48 text-sm border-2 p-2">
        <div className="sticky top-8">
        <h1>In the help:</h1>
        {nav.length > 0 && nav.map((n) => {
          if (n.id === "readme"){
            return;
          }
          return (
            <div key={n.id} className="m-2">
              <a href={`#${n.id}`}>
              {n.title}
              </a>
            </div>
          )
        })
        }
        </div>
      </nav>
      <div className="flex-1 help">
        {Markdoc.renderers.react(content, React, {})}
      </div>
    </div>
  )
}