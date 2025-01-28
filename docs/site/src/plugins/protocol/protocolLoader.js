// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

function getLinkByTitle(data, titleToMatch) {
  for (const item of data) {
    // Check the current item's title
    if (item.title === titleToMatch) {
      return item.link;
    }
    // Check in the nested items if they exist
    if (item.items && Array.isArray(item.items)) {
      const nestedLink = getLinkByTitle(item.items, titleToMatch);
      if (nestedLink) {
        return nestedLink; // Return if a match is found
      }
    }
  }
  return null; // Return null if no match is found
}

const protocolInject = async function (source) {
  this.cacheable && this.cacheable();

  const callback = this.async();
  const options = this.getOptions();

  const spec = options.protocolSpec.replace(/{/g, "&#123;");
  const lines = spec.split("\n");
  const toc = [];

  let currentCategory = null;
  let firstCategory = null;
  let contents = [];
  let output = [];
  lines.some((line, idx) => {
    if (line.startsWith("- [")) {
      const match = line.match(/\[(.*?)\]\((.*?)\)/);
      if (match) {
        const [_, title, link] = match;
        if (firstCategory === null) {
          firstCategory = title;
        }
        currentCategory = { title, link, items: [] };
        toc.push(currentCategory);
      }
    } else if (line.startsWith("    - [")) {
      // Sub-item (check for indentation)
      const match = line.match(/\[(.*?)\]\((.*?)\)/);
      if (match && currentCategory) {
        const [_, title, link] = match;
        currentCategory.items.push({ title, link });
      }
    }
    if (firstCategory && line.match(new RegExp(`#+ ${firstCategory}\\b`))) {
      contents = lines.slice(idx);
      return true;
    }
    return false;
  });
  let prevBlank = false;
  for (let i = 0; i < contents.length; i++) {
    if (!contents[i].match(/^<a name/) && !contents[i].match(/^<p align=/)) {
      if (contents[i].match(/^#+ /)) {
        const title = contents[i].replace(/#/g, "").trim();
        const link = getLinkByTitle(toc, title);
        if (link) {
          output.push(contents[i].trim() + ` {${link}}`);
        } else {
          output.push(contents[i].replace(/^# /, "### "));
        }
        prevBlank = false;
      } else {
        if (contents[i] !== "") {
          prevBlank = false;
          if (contents[i].includes("<a name=")) {
            output.push(contents[i]);
          } else {
            output.push(contents[i]);
          }
        } else if (!prevBlank) {
          prevBlank = true;
          output.push(contents[i]);
        }
      }
    }
  }
  //console.log(output);
  output = `<Protocol toc={${JSON.stringify(toc)}}/>\n${output.join("\n")}`;

  return callback && callback(null, source.replace(/<Protocol ?\/>/, output));
};

module.exports = protocolInject;
