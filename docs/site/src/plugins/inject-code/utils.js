// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Pass in an array of lines from code source.
// Return the array without blank lines in beg or end
// but leave whitespace indentation alone.
exports.trimContent = (content) => {
  let arr = content.split("\n");
  let start = 0;
  let end = arr.length;

  while (start < end && arr[start].trim() === "") {
    start++;
  }

  while (end > start && arr[end - 1].trim() === "") {
    end--;
  }

  return arr.slice(start, end).join("\n");
};

// Fix the spurious whitespace when copying code
// from the middle of source.
exports.removeLeadingSpaces = (codeText, prepend = "") => {
  codeText = codeText.replace(/^\n/, "");
  const numSpaces = codeText.match(/^\s*/)
    ? codeText.match(/^\s*/)[0].length
    : 0;
  if (numSpaces === 0) {
    return [prepend, codeText].join("\n");
  }
  const lines = codeText.split("\n");
  return [
    prepend,
    lines.map((line) => line.substring(numSpaces)).join("\n"),
  ].join("\n");
};

// Options are added to the @inject command by appending
// a space delimited list

// Remove comments. TODO: Add other langs
const removeComments = (text, options) => {
  const cont = options.some((element) =>
    element.toLowerCase().includes("nocomment"),
  );
  if (cont) {
    return text.replace(/^\s*\/\/.*$(?:\r\n?|\n)?/gm, "");
  } else {
    return text;
  }
};

const removeTests = (text, options) => {
  const cont = options.some((element) =>
    element.toLowerCase().includes("notest"),
  );
  if (cont) {
    console.log(text)
    return text
      .replace(/\s*#\[test.*?\n.*?}(?!;)\n?/gs, "\n{{plugin-removed-test}}\n")
      .replace(/\{\{plugin-removed-test\}\}\s*/gm, "");
  } else {
    return text;
  }
};

exports.processOptions = (text, options) => {
  // Replace all the //docs:: lines in code and license header
  let processed = text

    .replace(
      /^\/\/\s*Copyright.*Mysten Labs.*\n\/\/\s*SPDX-License.*?\n?$/gim,
      "",
    )
    .replace(/^\s*\/\/\s*docs::\/?.*\r?$\n?/gm, "");
  processed = removeComments(processed, options);
  processed = removeTests(processed, options);

  return processed;
};

// When including a function, struct by name
// Need to catch the stuff that comes before
// For example, comments, #[] style directives, and so on

exports.capturePrepend = (match, text) => {
  const numSpaces = match[1]
    ? match[1].match(/^\n/)
      ? match[1].length - 1
      : match[1].length
    : 0;
  let preFun = text.substring(0, match.index);
  const lines = preFun.split("\n");
  let pre = [];
  for (let x = lines.length - 1; x > 0; x--) {
    if (lines[x].trim() === "}" || lines[x].trim() === "") {
      break;
    } else {
      pre.push(lines[x].substring(numSpaces));
    }
  }
  return pre.reverse().join("\n");
};

// If opening and closing braces don't match
// there's a problem
exports.checkBracesBalance = (str) => {
  const openBraces = str.match(/{/g) || [];
  const closeBraces = str.match(/}/g) || [];

  return openBraces.length === closeBraces.length;
};

// Output codeblocks
exports.formatOutput = (language, title, content) => {
  return `\`\`\`${language} title="${title}"\n${content}\n\`\`\``;
};
