// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This component is used for embedding code files
// into {@include: } snippets
// It supports the same functionality as {@inject:}

const removeLeadingSpaces = (codeText, prepend = "") => {
  // Normalize newlines and drop *leading* blank lines only
  let text = codeText.replace(/\r\n?/g, "\n");
  while (text.startsWith("\n")) text = text.slice(1);

  const lines = text.split("\n");

  // Find minimal indentation among non-empty lines (spaces or tabs)
  let minIndent = Infinity;
  const indents = lines.map((line) => line.match(/^[ \t]*/)?.[0] ?? "");
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].trim() === "") continue; // ignore empty lines for baseline
    const indentLen = indents[i].length;
    if (indentLen < minIndent) minIndent = indentLen;
  }
  if (!isFinite(minIndent)) minIndent = 0; // all-blank content

  // Dedent safely: never strip more than a line actually has
  const body = lines
    .map((line, i) => line.slice(Math.min(minIndent, indents[i].length)))
    .join("\n");

  // Only add prepend when non-empty to avoid leading blank line
  return prepend ? `${prepend}\n${body}` : body;
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

// When including a function, struct by name
// Need to catch the stuff that comes before
// For example, comments, #[] style directives, and so on
// match is the regex match for particular code section
// match[1] is the (\s*) capture group to count indentation
// text is all the code in the particular file
const capturePrepend = (match, text) => {
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

function captureBalanced(sub, open = "{", close = "}") {
  let depth = 0;
  for (let i = 0; i < sub.length; i++) {
    const ch = sub[i];
    if (ch === open) depth++;
    else if (ch === close) {
      depth--;
      if (depth === 0) return sub.slice(0, i + 1);
    }
  }
  return null;
}

function escapeRegex(s) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

const cutAtBodyStart = (text) => {
  const brace = text.indexOf("{");
  if (brace !== -1) return text.slice(0, brace).trimEnd();
  const semi = text.indexOf(";");
  return semi !== -1 ? text.slice(0, semi + 1).trimEnd() : text.trimEnd();
};

exports.returnFunctions = (source, functions, language, sig) => {
  if (!functions) {
    return source;
  }
  const funs = functions
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
  let funContent = [];
  for (let fn of funs) {
    fn = fn.trim();
    let funStr = "";

    if (language === "move") {
      // Robust Move function header:
      // optional: public | public(friend) | public(package)
      // optional: entry
      // fun <name> ... {
      const headerRe = new RegExp(
        String.raw`^(\s*)(?:public(?:\s*\(\s*(?:friend|package)\s*\))?\s+)?(?:entry\s+)?fun\s+${escapeRegex(fn)}\b[\s\S]*?\{`,
        "m",
      );
      const m = headerRe.exec(source);
      if (!m) continue;
      const startIdx = m.index;
      const sub = source.slice(startIdx);
      const bracePos = sub.indexOf("{");
      if (bracePos === -1) continue;
      const block = captureBalanced(sub.slice(bracePos));
      if (!block) continue;
      const header = sub.slice(0, bracePos);
      const full = header + block;
      const pre = capturePrepend(m, source);
      const out = sig ? cutAtBodyStart(full) : full;
      funContent.push(removeLeadingSpaces(out, pre));
      continue;
    } else if (language === "ts") {
      funStr = `^(\\s*)(async )?(export (default )?)?function \\b${escapeRegex(fn)}\\b[\\s\\S]*?\\n\\1\\}`;
    } else if (language === "rust") {
      funStr = `^(\\s*)(?:pub\\s+)?(?:async\\s+)?(?:const\\s+)?(?:unsafe\\s+)?(?:extern\\s+(?:\"[^\"]+\"\\s*)?)?fn\\s+${escapeRegex(fn)}\\s*(?:<[^>]*>)?\\s*\\([^)]*\\)\\s*(?:->\\s*[^;{]+)?\\s*(?:;|\\{[\\s\\S]*?^\\1\\})`;
    }

    if (funStr) {
      const funRE = new RegExp(funStr, "ms");
      const funMatch = funRE.exec(source);
      if (funMatch) {
        let pre = capturePrepend(funMatch, source);
        let matched = funMatch[0];
        if (sig) {
          matched = cutAtBodyStart(matched);
        }
        funContent.push(removeLeadingSpaces(matched, pre));
      }
    }
  }
  return funContent
    .join("\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
};

exports.returnTag = (source, tag) => {
  // Capture the content between closing and opening docs tags.
  // Account for any )}; characters that might be added to the closing tag.
  // These characters are used to add closing syntax - useful when
  // you want to capture only first part of a code snippet.
  // Intentionally forcing the closing docs tag.
  if (!tag) return source;
  const docTagRe = new RegExp(
    `\\/\\/\\s?docs::#${escapeRegex(tag)}\\b[^\\n]*\\n([\\s\\S]*)\\/\\/\\s*docs::\\/\\s?#${escapeRegex(tag)}\\b(?<closers>[)};]*)`,
    "m",
  );
  const matchTaggedContent = docTagRe.exec(source);
  if (!matchTaggedContent) {
    return {
      ok: false,
      content: `// Section '${tag}' not found or is not closed properly`,
    };
  }
  let taggedContent = matchTaggedContent[1];

  const pauseTagRe = new RegExp(
    `^[\\t ]*\\/\\/[\\t ]*docs::#${escapeRegex(tag)}-pause[\\t ]*$[\\s\\S]*?^[\\t ]*\\/\\/[\\t ]*docs::#${escapeRegex(tag)}-resume[\\t ]*\\n?`,
    "gm",
  );

  taggedContent = taggedContent.replace(pauseTagRe, "");

  const closers =
    (matchTaggedContent.groups && matchTaggedContent.groups.closers) ||
    matchTaggedContent[2] ||
    "";
  var closing = "";
  // Add the optional closing characters with proper spacing.
  if (/[)};]+/.test(closers)) {
    const closingTotal = closers.length;
    let closingArray = [];
    for (let i = 0; i < closingTotal; i++) {
      const currentChar = closers[i];
      const nextChar = closers[i + 1];

      if (nextChar === ";") {
        closingArray.push(currentChar + nextChar);
        i++;
      } else {
        closingArray.push(currentChar);
      }
    }
    const totClosings = closingArray.length;

    // Process any closing elements added in the closing comment of source code
    for (let j = 0; j < totClosings; j++) {
      let space = "  ".repeat(totClosings - 1 - j);
      closing += `\n${space}${closingArray[j]}`;
    }
  }
  taggedContent = removeLeadingSpaces(taggedContent + closing);

  return taggedContent;
};

exports.returnVariables = (source, variables, language) => {
  if (!variables) return source;
  const names = variables.split(",");
  let out = [];
  if (language === "ts") {
    const varTsFunction = `^( *)?.*?(let|const) \\b${escapeRegex(variables)}\\b.*=>`;
    const varTsVariable = `^( *)?.*?(let|const) \\b${escapeRegex(variables)}\\b (?!.*=>)=.*;`;
    const reFun = new RegExp(varTsFunction, "m");
    const reVar = new RegExp(varTsVariable, "m");
    const mFun = reFun.exec(source);
    const mVar = reVar.exec(source);
    if (mFun) {
      const start = source.slice(mFun.index);
      const endText = `^${mFun[1] ? mFun[1] : ""}\\)?\\};`;
      const endRE = new RegExp(endText, "m");
      const endMatch = endRE.exec(start);
      let pre = capturePrepend(mFun, source);
      out.push(
        removeLeadingSpaces(
          start.slice(0, endMatch.index + endMatch[0].length),
          pre,
        ),
      );
    } else if (mVar) {
      let pre = capturePrepend(mVar, source);
      out.push(removeLeadingSpaces(mVar[0], pre));
    } else {
      source =
        "Variable not found. If code is formatted correctly, consider using code comments instead.";
    }
  } else {
    for (let v of names) {
      v = v.trim();
      const shortRe = new RegExp(
        `^(\\s*)?(#\\[test_only\\])?(let|const) \\(?.*?\\b${escapeRegex(v)}\\b.*?\\)?\\s?=.*;`,
        "m",
      );
      const longRe = new RegExp(
        `^(\\s*)?(#\\[test_only\\])?(let|const) \\(?.*?\\b${escapeRegex(v)}\\b.*?\\)?\\s?= \\{[^}]*\\};\\s*$`,
        "m",
      );
      const mShort = shortRe.exec(source);
      const mLong = longRe.exec(source);
      const m = mShort || mLong;
      if (m) {
        let pre = capturePrepend(m, source);
        out.push(removeLeadingSpaces(m[0], pre));
      } else {
        return "Variable not found. If code is formatted correctly, consider using code comments instead.";
      }
    }
  }
  return out.join("\n").trim();
};

exports.returnStructs = (source, structList, language) => {
  if (!structList) return source;
  const names = structList
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
  const src = source.replace(/\r\n?/g, "\n");
  const out = [];

  for (const name of names) {
    const shortStructRE = new RegExp(
      String.raw`^\s*(?:pub(?:lic)?(?:\s*\(\s*[^)]+\s*\))?\s+)?struct\s+${escapeRegex(name)}\s*;[ \t]*(?:\r?\n|$)`,
    );

    const m = shortStructRE.exec(src);
    let full,
      pre = "";
    if (!m) {
      const structBegRE = new RegExp(
        String.raw`^\s*(?:pub(?:lic)?(?:\s*\(\s*[^)]+\s*\))?\s+)?struct\s+${escapeRegex(name)}\b[\s\S]*?\{`,
        "m",
      );
      const ml = structBegRE.exec(src);
      if (!ml) {
        return "Struct not found. If code is formatted correctly, consider using code comments instead.";
      } else {
        const startIdx = ml.index;
        const sub = src.slice(startIdx);

        // headerRe included the first `{`, so find its position in `sub`
        const bracePos = sub.indexOf("{");
        if (bracePos === -1) {
          return "Struct not found. If code is formatted correctly, consider using code comments instead.";
        }

        const block = captureBalanced(sub.slice(bracePos));
        if (!block) {
          return "Struct not found. If code is formatted correctly, consider using code comments instead.";
        }

        full = sub.slice(0, bracePos) + block; // header through matching `}`
        pre = capturePrepend(ml, src);
      }
    } else {
      full = m[0];
      pre = capturePrepend(m, src);
    }
    out.push(removeLeadingSpaces(full, pre));
  }

  return out.join("\n").trim();
};

exports.returnTypes = (source, type) => {
  if (!type) return source;
  const types = type.split(",");
  let out = [];
  for (let t of types) {
    const startRe = new RegExp(
      `^( *)(export )?type \\b${escapeRegex(t)}\\b`,
      "m",
    );
    const m = startRe.exec(source);
    if (m) {
      let sub = source.slice(m.index);
      const spaces = m[1] || "";
      const endRe = new RegExp(`^${spaces}\\};`, "m");
      const e = endRe.exec(sub);
      if (e) out.push(removeLeadingSpaces(sub.slice(0, e.index + e[0].length)));
      else out.push("Error capturing type declaration.");
    }
  }
  return out.join("\n").trim();
};

exports.returnTraits = (source, trait) => {
  if (!trait) return source;
  const traits = trait.split(",");
  const out = [];
  for (let t of traits) {
    t = t.trim();
    // Match the header line robustly: optional pub/public, name, any suffix on the same line,
    // up to and including the first `{` (bounds, generics, where-clause tolerated).
    const headerRe = new RegExp(
      String.raw`^(\s*)(?:pub(?:lic)?\s+)?trait\s+${escapeRegex(t)}\b[^\n]*\{`,
      "m",
    );
    const m = headerRe.exec(source);
    if (!m) {
      return "Trait not found. If code is formatted correctly, consider using code comments instead.";
    }
    const startIdx = m.index;
    const sub = source.slice(startIdx);
    const braceStart = sub.indexOf("{");
    if (braceStart === -1) {
      return "Trait not found. If code is formatted correctly, consider using code comments instead.";
    }
    const block = captureBalanced(sub.slice(braceStart));
    if (!block) {
      return "Trait not found. If code is formatted correctly, consider using code comments instead.";
    }
    const full = sub.slice(0, braceStart) + block; // header .. matched closing }
    const pre = capturePrepend(m, source);
    out.push(removeLeadingSpaces(full, pre));
  }
  return out.join("\n").trim();
};

exports.returnImplementations = (source, impl) => {
  if (!impl) return source;
  const impls = impl.split(",");
  const out = [];
  for (const imp of impls) {
    const implRE = new RegExp(
      String.raw`^(\s*)(?:\uFEFF)?\s*impl(?:\s*<[\s\S]*?>)?\s+` +
      String.raw`(?:` +
        // A) impl <Trait> for <Type> { ... } where the searched name is the TRAIT
        String.raw`(?:(?:[\w:]+::)*${escapeRegex(imp)}(?:\s*<[\s\S]*?>)?\s+for\s+(?<type>[\s\S]*?)(?:\s+where\s+[\s\S]*?)?)` +
        String.raw`|` +
        // B) impl <Trait> for <Type> { ... } where the searched name is the TYPE
        String.raw`(?:(?<trait>[\s\S]*?)\s+for\s+(?:[\w:]+::)*${escapeRegex(imp)}(?:\s*<[\s\S]*?>)?(?:\s+where\s+[\s\S]*?)?)` +
        String.raw`|` +
        // C) impl <Type> { ... }  (inherent impl) where the searched name is the TYPE
        String.raw`(?:(?:[\w:]+::)*${escapeRegex(imp)}(?:\s*<[\s\S]*?>)?(?:\s+where\s+[\s\S]*?)?)` +
      String.raw`)\s*\{`,
      'ms'
    );

    const m = implRE.exec(source);
    if (!m) {
      return "Implementation block match not found. If code is formatted correctly, consider using code comments instead.";
    }
    const startIdx = m.index;
    const sub = source.slice(startIdx);
    const braceStart = sub.indexOf("{");
    if (braceStart === -1) {
      return "Implementation block not found. If code is formatted correctly, consider using code comments instead.";
    }
    const block = captureBalanced(sub.slice(braceStart));
    if (!block) {
      return "Implementation block not found. If code is formatted correctly, consider using code comments instead.";
    }
    const full = sub.slice(0, braceStart) + block; // header .. matched closing }
    const pre = capturePrepend(m, source);
    out.push(removeLeadingSpaces(full, pre));
  }
  return out.join("\n").trim();
}

exports.returnEnums = (source, enumVal) => {
  if (!enumVal) return source;
  const enums = enumVal
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
  const out = [];
  for (const e of enums) {
    // Match optional keywords: export / declare / const (TS) OR pub (Rust)
    const re = new RegExp(
      `^(\\s*)(?:export\\s+)?(?:declare\\s+)?(?:const\\s+)?(?:pub(?:lic)?(?:\$begin:math:text$package\\$end:math:text$)?\\s+)?enum\\s+${escapeRegex(e)}\\b(?:\\s*<[^>]*>)?(?:\\s+has\\s+[^{]+)?\\s*\\{`,
      "m",
    );
    const m = re.exec(source);
    if (m) {
      const start = m.index;
      const sub = source.slice(start);
      const openIdx = sub.indexOf("{");
      if (openIdx !== -1) {
        const block = captureBalanced(sub.slice(openIdx));
        if (block) {
          out.push(removeLeadingSpaces(sub.slice(0, openIdx) + block));
        }
      }
    }
  }
  return out.join("\n").trim();
};

exports.returnModules = (source, module) => {
  const modStr = `^(\\s*)*module \\b${escapeRegex(module)}\\b.*?}\\n(?=\\n)?`;
  const modRE = new RegExp(modStr, "msi");
  const modMatch = modRE.exec(source);
  if (modMatch) {
    const abridged = source.substring(modMatch.index);
    const lines = abridged.split("\n");
    let open = [];
    let close = [];
    let modLines = [];
    for (let line of lines) {
      modLines.push(line);
      open = [...open, ...(line.match(/{/g) || [])];
      close = [...close, ...(line.match(/}/g) || [])];
      if (open.length !== 0 && close.length === open.length) {
        break;
      }
    }
    const preMod = capturePrepend(modMatch, source);
    return removeLeadingSpaces(modLines.join("\n"), preMod);
  } else {
    return "Module not found. If code is formatted correctly, consider using code comments instead.";
  }
};

exports.returnComponents = (source, component) => {
  const components = component.split(",");
  let componentContent = [];
  for (let comp of components) {
    let names = [];
    let name = comp;
    let element = "";
    let ordinal = "";
    if (comp.indexOf(":") > 0) {
      names = comp.split(":");
      name = names[0];
      element = names[1];
      ordinal = names[2] ? names[2] : "";
    }
    const compStr = `^( *)(export (default )?)?function \\b${name}\\b.*?\\n\\1\\}\\n`;
    const compRE = new RegExp(compStr, "ms");
    const compMatch = compRE.exec(source);
    if (compMatch) {
      if (element) {
        const elStr = `^( *)\\<${element}\\b.*?>.*?\\<\\/${element}>`;
        const elRE = new RegExp(elStr, "msg");
        let elementsToKeep = [1];
        if (ordinal) {
          if (ordinal.indexOf("-") > 0 && ordinal.indexOf("&") > 0) {
            console.log(
              "Only dashes or commas allowed for selecting component elements, not both.",
            );
          } else {
            if (ordinal.indexOf("-") > 0) {
              const [start, end] = ordinal.split("-").map(Number);
              elementsToKeep = Array.from(
                { length: end - start + 1 },
                (_, i) => start + i,
              );
            }
            if (ordinal.indexOf("&") > 0) {
              elementsToKeep = ordinal.split("&").map(Number);
            }
          }
        }
        elementsToKeep.sort((a, b) => a - b);
        for (let x = 0; x < elementsToKeep[elementsToKeep.length - 1]; x++) {
          const elMatch = elRE.exec(compMatch);
          if (elementsToKeep.includes(x + 1)) {
            componentContent.push(removeLeadingSpaces(elMatch[0]));
          } else {
            if (x > 0 && componentContent[x - 1].trim() !== "...") {
              componentContent.push("\n...");
            }
          }
        }
      } else {
        let preComp = utils.capturePrepend(compMatch, source);
        componentContent.push(removeLeadingSpaces(compMatch[0], preComp));
      }
    }
  }
  return componentContent.join("\n").trim();
};

exports.returnDeps = (source, dep) => {
  const deps = dep
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
  const out = [];

  // Token for a Move path segment: address (0x...) or identifier
  const seg = String.raw`(?:0x[0-9a-fA-F]+|[A-Za-z_][A-Za-z0-9_]*)`;
  const pathJoin = (p) => p.map(escapeRegex).join(String.raw`\s*::\s*`);

  for (let d of deps) {
    // Split on :: to detect an optional terminal item
    const parts = d
      .split("::")
      .map((s) => s.trim())
      .filter(Boolean);
    let baseParts = parts;
    let item = null;
    if (parts.length >= 2) {
      baseParts = parts.slice(0, -1);
      item = parts[parts.length - 1];
    }

    // If someone passed only an item, treat base as wildcard (unlikely but safe)
    if (baseParts.length === 0) baseParts = [String.raw`${seg}`];

    const basePath = pathJoin(baseParts);

    // Build pattern head: ^  [indent]  (#[test_only])?  use  basePath
    const head = String.raw`^(\s*)(?:#\[\s*test_only\s*\]\s*)?use\s+${basePath}`;

    let body;
    if (item) {
      const itemName = escapeRegex(item);
      // Either ...::Item [as Alias]?   OR   ...::{ ... Item ... }
      body = String.raw`(?:\s*::\s*${itemName}\b(?:\s+as\s+${seg})?|\s*::\s*\{[\s\S]*?\b${itemName}\b[\s\S]*?\})`;
    } else {
      // No specific item: accept either a further nested path or a brace group
      body = String.raw`(?:\s*::\s*(?:${seg}(?:\s*::\s*${seg})*)|\s*::\s*\{[\s\S]*?\})?`;
    }

    const tail = String.raw`\s*;`;
    const useStr = head + body + tail;
    const useRE = new RegExp(useStr, "ms");
    const m = useRE.exec(source);

    if (m) {
      const pre = capturePrepend(m, source);
      out.push(removeLeadingSpaces(m[0], pre));
    } else {
      return "Use statement not found. If code is formatted correctly, consider using code comments instead.";
    }
  }

  return out.join("\n").trim();
};

exports.returnNotests = (source) => {
  return source
    .replace(/\s*#\[test.*?\n.*?(}(?!;)\n?|$)/gs, "\n{{plugin-removed-test}}\n")
    .replace(/\{\{plugin-removed-test\}\}\s*/gm, "");
};

exports.highlightLine = (source, highlightTerm) => {
  const lines = source.split("\n");
  const matchingLines = lines
    .map((line, idx) => (line.includes(highlightTerm) ? idx + 1 : null))
    .filter((n) => n !== null);

  return matchingLines.join(",");
};
