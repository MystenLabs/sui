// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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

const isOption = (opts, option) => {
  return option
    ? opts.some((element) => element.toLowerCase().includes(option))
    : false;
};

// Remove comments. TODO: Add other langs
const removeComments = (text, options) => {
  if (isOption(options, "nocomment")) {
    return text.replace(/^ *\/\/.*\n/gm, "");
  } else {
    return text;
  }
};

const removeTests = (text, options) => {
  if (isOption(options, "notest")) {
    const processed = text
      .replace(
        /\s*#\[test.*?\n.*?(}(?!;)\n?|$)/gs,
        "\n{{plugin-removed-test}}\n",
      )
      .replace(/\{\{plugin-removed-test\}\}\s*/gm, "");
    return processed;
  } else {
    return text;
  }
};

// Remove double spaces from output when changing the code is not preferred.
const singleSpace = (text, options) => {
  if (isOption(options, "singlespace")) {
    const processed = text.replace(/^\s*[\r\n]/gm, "");
    return processed;
  } else {
    return text;
  }
};

// Remove blank lines from beginning and end of code source
// but leave whitespace indentation alone. Also, replace multiple
// blank lines that occur in succession.
const trimContent = (content) => {
  let arr = content.split("\n");
  const filtered = arr.filter((line, index) => {
    return (
      line.trim() !== "" ||
      (line.trim() === "" && arr[index - 1] && arr[index - 1].trim() !== "")
    );
  });
  let start = 0;
  let end = filtered.length;

  while (start < end && filtered[start].trim() === "") {
    start++;
  }

  while (end > start && filtered[end - 1].trim() === "") {
    end--;
  }

  return filtered.slice(start, end).join("\n");
};

exports.processOptions = (text, options) => {
  // Replace all the //docs:: lines in code and license header
  let processed = text
    .replace(
      /^\/\/\s*Copyright.*Mysten Labs.*\n\/\/\s*SPDX-License.*?\n?$/gim,
      "",
    )
    .replace(/^\s*\/\/\s*docs::\/?.*\r?$\n?/gm, "")
    .replace(
      /sui\s?=\s?{\s?local\s?=.*sui-framework.*/i,
      'Sui = { git = "https://github.com/MystenLabs/sui.git", subdir = "crates/sui-framework/packages/sui-framework", rev = "framework/testnet" }',
    );
  processed = removeComments(processed, options);
  processed = removeTests(processed, options);
  processed = singleSpace(processed, options);
  processed = trimContent(processed);

  return processed;
};

// When including a function, struct by name
// Need to catch the stuff that comes before
// For example, comments, #[] style directives, and so on
// match is the regex match for particular code section
// match[1] is the (\s*) capture group to count indentation
// text is all the code in the particular file
exports.capturePrepend = (match, text) => {
  const numSpaces =
    Array.isArray(match) && match[1] ? match[1].replace(/\n/, "").length : 0;
  let preText = text.substring(0, match.index);
  const lines = preText.split("\n");
  let pre = [];
  for (let x = lines.length - 1; x > 0; x--) {
    if (
      lines[x].match(/^ *\//) ||
      lines[x].match(/^ *\*/) ||
      lines[x].match(/^ *#/) ||
      lines[x].trim() === ""
    ) {
      // Capture sometimes incorrectly includes a blank line
      // before function/struct. Don't include.
      if (!(lines[x].trim() === "" && x === lines.length - 1)) {
        pre.push(lines[x].substring(numSpaces));
      }
    } else {
      break;
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
exports.formatOutput = (language, title, content, options) => {
  if (options && isOption(options, "notitle")) {
    return `\`\`\`${language}\n${content.replace(/\t/g, "  ")}\n\`\`\``;
  }
  return `\`\`\`${language} title="${title}"\n${content.replace(/\t/g, "  ")}\n\`\`\``;
};
