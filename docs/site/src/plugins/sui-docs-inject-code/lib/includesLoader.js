"use strict";
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_1 = __importDefault(require("fs"));
const path_1 = __importDefault(require("path"));
const markdownLoader = function (source) {
    let fileString = source;
    const callback = this.async();
    const options = this.getOptions();
    const markdownFilename = path_1.default.basename(this.resourcePath);
    const markdownFilepath = path_1.default.dirname(this.resourcePath);
    const repoPath = path_1.default.join(__dirname, "../../../../../..");
    // Do not load and render markdown files without docusaurus header.
    // These files are only used to be included in other files and should not generate their own web page
    if (fileString.length >= 3 && fileString.substring(0, 3) !== "---") {
        return callback && callback(null, "");
    }
    function addMarkdownIncludes(fileContent) {
        let res = fileContent;
        const matches = fileContent.match(/\{@\w+: .+\}/g);
        if (matches) {
            matches.forEach((match) => {
                const replacer = new RegExp(match, "g");
                if (match.startsWith("{@inject: ")) {
                    const injectFileFull = match.substring(10, match.length - 1);
                    const injectFile = injectFileFull.substring(0, injectFileFull.indexOf("#") > 0
                        ? injectFileFull.indexOf("#")
                        : injectFileFull.length);
                    let fileExt = injectFile.substring(injectFile.lastIndexOf(".") + 1);
                    let language = "";
                    const fullPath = path_1.default.join(repoPath, injectFile);
                    switch (fileExt) {
                        case "move":
                            language = "rust";
                            break;
                        case "toml":
                            language = "rust";
                            break;
                        case "lock":
                            language = "rust";
                            break;
                        case "sh":
                            language = "shell";
                            break;
                        case "mdx":
                            language = "markdown";
                            break;
                        case "tsx":
                            language = "ts";
                            break;
                        default:
                            language = fileExt;
                    }
                    if (fs_1.default.existsSync(fullPath)) {
                        let injectFileContent = fs_1.default.readFileSync(fullPath, "utf8");
                        const marker = injectFileFull.indexOf("#") > 0
                            ? injectFileFull.substring(injectFileFull.indexOf("#"))
                            : null;
                        if (marker) {
                            const regexStr = `\\/\\/\\s?docs::${marker.trim()}\\b([\\s\\S]*)\\/\\/\\s*docs::\\/\\s?${marker.trim()}\\b`;
                            const closingsStr = `\\/\\/\\s?docs::\\/${marker.trim()}\\b([)};]*)`;
                            const closingRE = new RegExp(closingsStr, "g");
                            const regex = new RegExp(regexStr, "g");
                            const match = regex.exec(injectFileContent);
                            const closingStr = closingRE.exec(injectFileContent);
                            if (match) {
                                injectFileContent = match[1];
                            }
                            if (closingStr) {
                                const closingTotal = closingStr[1].length;
                                let closingArray = [];
                                for (let i = 0; i < closingTotal; i++) {
                                    const currentChar = closingStr[1][i];
                                    const nextChar = closingStr[1][i + 1];
                                    if (nextChar === ";") {
                                        closingArray.push(currentChar + nextChar);
                                        i++;
                                    }
                                    else {
                                        closingArray.push(currentChar);
                                    }
                                }
                                const totClosings = closingArray.length;
                                //let closing = `${'\t'.repeat(totClosings - 1)}${closingArray[0]}`;
                                let closing = "";
                                for (let j = 0; j < totClosings; j++) {
                                    let space = "  ".repeat(totClosings - 1 - j);
                                    closing += `\n${space}${closingArray[j]}`;
                                }
                                injectFileContent = injectFileContent.trim() + closing;
                            }
                            // Start searching for the pause doc comments
                            // Must be in form: // docs::#idName-pause: optional replacement text
                            // Must be followed at some point by: // docs::#idName-resume
                            // Replace text in the middle with optional replacement text or blankness
                            const pauseStr = `\\/\\/\\s?docs::${marker.trim()}-pause:?(.*)`;
                            const pauseRE = new RegExp(pauseStr, "g");
                            let matches = injectFileContent.match(pauseRE);
                            for (let match in matches) {
                                const test = matches[match].replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
                                let replacer = "";
                                if (matches[match].indexOf("-pause:") > 0) {
                                    replacer = matches[match].substring(matches[match].indexOf("-pause") + 8);
                                }
                                const newRE = new RegExp(test);
                                const resumeStr = `\\/\\/\\s?docs::${marker.trim()}-resume`;
                                const resumeRE = new RegExp(resumeStr);
                                let paused;
                                if (replacer !== "") {
                                    paused = new RegExp(newRE.source.replace(":?(.*)", "") +
                                        ".*?" +
                                        resumeRE.source, "gs");
                                }
                                else {
                                    paused = new RegExp(newRE.source.replace(":?(.*)", "") +
                                        "(?!:).*?" +
                                        resumeRE.source, "gs");
                                }
                                injectFileContent = injectFileContent.replace(paused, replacer);
                            }
                        }
                        // Replace all the //docs:: lines in code
                        injectFileContent = injectFileContent.replace(/^\s*\/\/\s*docs::\/?.*\r?$\n?/gm, "");
                        injectFileContent = `\`\`\`${language} title=${injectFile}\n${injectFileContent}\n\`\`\``;
                        res = res.replace(replacer, injectFileContent);
                        res = addMarkdownIncludes(res);
                    }
                    else {
                        res = res.replace(replacer, `\n> code to inject not found: ${injectFile} --> ${fullPath}\n`);
                    }
                }
                else {
                    const parts = match.substring(2, match.length - 3).split(": ");
                    if (parts.length === 2) {
                        if (options.embeds) {
                            for (const embed of options.embeds) {
                                if (embed.key === parts[0]) {
                                    const embedResult = embed.embedFunction(parts[1]);
                                    res = res.replace(replacer, embedResult);
                                }
                            }
                        }
                    }
                }
            });
        }
        return res;
    }
    function replacePlaceHolders(documentContent) {
        var res = documentContent;
        if (options.replacements) {
            var placeHolders = [...options.replacements];
            if (!placeHolders) {
                placeHolders = [];
            }
            placeHolders.push({
                key: "{ContainerMarkdown}",
                value: markdownFilename,
            });
            placeHolders.forEach((replacement) => {
                const replacer = new RegExp(replacement.key, "g");
                res = res.replace(replacer, replacement.value);
            });
        }
        return res;
    }
    fileString = replacePlaceHolders(addMarkdownIncludes(fileString));
    return callback && callback(null, fileString);
};
exports.default = markdownLoader;
