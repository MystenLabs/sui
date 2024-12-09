// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Custom Prism.js extension for Move;
 * See extension guide here: https://prismjs.com/extending.html
 */

// tokens supported by the syntax theme:
// namespace
// string,
// attr-value
// punctuation,
// operator
// entity,
// url,
// symbol,
// number,
// boolean,
// variable,
// constant,
// property,
// regex,
// inserted
// atrule,
// keyword,
// attr-name,
// function,
// deleted,
// tag,
// selector,
// important,
// function,
// bold,
// italic

(function (Prism) {
  var multilineComment = /\/\*(?:[^*/]|\*(?!\/)|\/(?!\*)|<self>)*\*\//.source;
  for (var i = 0; i < 2; i++) {
    // support 4 levels of nested comments
    multilineComment = multilineComment.replace(/<self>/g, function () {
      return multilineComment;
    });
  }
  multilineComment = multilineComment.replace(/<self>/g, function () {
    return /[^\s\S]/.source;
  });

  Prism.languages["move"] = {
    comment: [
      {
        name: "line-comments",
        pattern: RegExp(/(^|[^\\])/.source + multilineComment),
        lookbehind: true,
        greedy: true,
      },
      {
        name: "block-comments",
        pattern: /(^|[^\\:])\/\/.*/,
        lookbehind: true,
        greedy: true,
      },
    ],

    /**
     * Scope: Module
     */

    "module-header": {
      pattern: /\b(module)\s+(\w+)::(\w+)\s*[{;]/,
      inside: {
        "module-keyword": {
          pattern: /\b(module)\b/,
          alias: "keyword",
        },
        "module-name": {
          pattern: /(\b(?:module)\s+)(\w+)/,
          lookbehind: true,
          greedy: true,
          alias: "entity",
        },
        "double-colon": {
          pattern: /::/,
          alias: "punctuation",
        },
      }
    },

    /**
     * Scope: Module:Use
     */

    "use-import-fun": {
      pattern: /\b(use).*;/,
      inside: {
        "use-keyword": {
          pattern: /\b(use)\b/,
          alias: "keyword",
        },
        "fun-keyword": {
          pattern: /\b(fun)\b/,
          alias: "keyword",
        },
        "as-keyword": {
          pattern: /\b(as)\b/,
          alias: "keyword",
        },
        "self-keyword": {
          pattern: /\b(Self)\b/,
          alias: "variable",
        },
        "double-colon": {
          pattern: /::/,
          alias: "punctuation",
        },
        "use-address": {
          pattern: /((0x[0-9A-F]+)|([a-z][a-z_0-9]+))/,
          alias: "address-alias",
          // lookbehind: true,
          // alias: "constant",
        },
      }
    },

    /**
     * Scope: Module:Friend
     */
    "friend": {
      pattern: /\b(friend).*;/,
      inside: {
        "friend-keyword": {
          pattern: /\b(friend)\b/,
          alias: "keyword",
        },
        "double-colon": {
          pattern: /::/,
          alias: "punctuation",
        },
        "friend-address": {
          pattern: /((0x[0-9A-F]+)|([a-z][a-z_0-9]+))/,
          alias: "address-alias",
        },
      }
    },

    /**
     * Scope: Module:Const
     */

    "const": {
      pattern: /\b(const)\b.*(?=;)/,
      greedy: true,
      // alias: "keyword",
      inside: {
        "const-keyword": {
          pattern: /\b(const)\b/,
          alias: "keyword",
        },
        "error-const-name": {
          pattern: /\b(E\w+)(?=\s*:)/,
          greedy: true,
          alias: ["constant", "abort-code"],
        },
        "const-name": {
          pattern: /\b\w+\b/,
          greedy: true,
          alias: "constant",
        },
        "const-type": {
          pattern: /\b(u8|u16|u32|u64|u128|u256|bool|address|vector)\b/,
          alias: "keyword",
        },
        // see assignments in the end of this file
        // Prism allows injecting tokens into other tokens
        literals: null
      }
    },

    /**
     * Scope: Module:Struct
     */

    "struct-or-enum-definition": {
      pattern: /\b(struct|enum).*[{;]/,
      inside: {
        "enum-keyword": {
          pattern: /\b(enum)\b/,
          alias: "keyword",
        },
        "struct-keyword": {
          pattern: /\b(struct)\b/,
          alias: "keyword",
        },
        "struct-has": {
          pattern: /\b(has)\b/,
          alias: "keyword",
        },
        "abilities": {
          pattern: /\b(key|store|copy|drop)\b/,
          alias: ["regex", "important"],
        },
        "struct-name": {
          pattern: /(\w+)/,
          lookbehind: true,
          alias: "entity"
        },
        "phantom-keyword": {
          pattern: /\b(phantom)\b/,
          alias: "keyword",
        },
      }
    },

    /**
     * Scope: Module:Function
     */

    "function-name": {
      pattern: /(\b(?:fun)\s+)(\w+)/,
      lookbehind: true,
      greedy: true,
      alias: "function",
    },

    "function-keyword": {
      pattern: /\b(fun)\b/,
      alias: "keyword",
    },

    "visibility": {
      pattern: /\b(public|entry|native|\(friend\))\b/,
      alias: "keyword"
    },

    "macro": {
      pattern: /\b(macro)\b/,
      alias: "keyword",
    },

    /**
     * Scope: Function
     */

    "macro-variable": {
      pattern: /[$](\w+)/,
      alias: "builtin",
    },

    "let-statement": {
      pattern: /(\b(?:let)\s+(?:mut\s+)?)([a-z_]+)/,
      lookbehind: true,
      alias: "variable",
    },

    "let-keyword": {
      pattern: /\b(let)\b/,
      alias: "keyword",
    },

    "mut-keyword": {
      pattern: /\b(mut)\b/,
      alias: "keyword",
    },

    "macro-call": {
      pattern: /\b(\w+)!/,
      alias: "builtin",
    },

    "control-flow": {
      pattern: /\b(?:abort|break|continue|return|if|while)\b/,
      alias: "keyword",
    },

    "label": {
      pattern: /'(\w+):/,
      alias: "namespace",
    },

    "function-call": {
      pattern: /\b([a-z][a-z_]+)(?=\s*[<(])/,
      greedy: true,
      alias: "function",
    },

    /**
     * Scope: Top | Module
     */

    "attribute": {
      pattern: /#!?\[(?:[^\[\]"]|"(?:\\[\s\S]|[^\\"])*")*\]/,
      greedy: true,
      alias: "attr-name",
    },

    /**
     * Scope: Misc (Any)
     */

    "abilities": {
      pattern: /\b(key|store|copy|drop)\b/,
      alias: ["regex", "important"],
    },

    "built-in-types": {
      pattern: /\b(bool|address|u8|u16|u32|u64|u128|u256|vector)\b/,
      alias: "keyword",
    },

    /** Just some, non-object, most commonly used */
    "sui-native-types": {
      pattern: /\b(Option|String|UID|ID|VecSet|VecMap)\b/,
      alias: "symbol",
    },

    "mut-reference": {
      pattern: /(&)(mut)\b/,
      lookbehind: true,
      alias: "keyword",
    },

    "literals": [
      /** ASCII Bytestring literal: b"this is ascii" */
      {
        pattern: /(?:b)"(\\[\s\S]|[^\\"])*"/,
        lookbehind: true,
        alias: "string",
      },
      /** HEX Bytestring literal: x"AF" */
      {
        pattern: /(?:x)"([0-9A-F]+)"/,
        lookbehind: true,
        alias: "string",
      },
      /** Boolean literal: true */
      {
        pattern: /\b(?:true|false)\b/,
        alias: "boolean",
      },
      /** Number literal: 10, 10u8 */
      {
        pattern: /\b\d[\d_]*(u8|u16|u32|u64|u128|u256)?\b/,
        alias: "number",
      },
      /** Address literal */
      {
        pattern: /@0x[0-9A-F]+/,
        alias: "constant",
      },
      /** Numeric HEX literal */
      {
        pattern: /0x[0-9A-F]+/,
        alias: "number",
      },
    ],

    "property-name": {
      pattern: /(\w+)(?=\s*:\s*[^:])/,
      alias: ["function-argument", "struct-property-name"],
    },

    "property-value": {
      pattern: /(:\s*(?:(&mut|&)\s*)?)(\w+)/,
      lookbehind: true,
      // alias: "property",
    },

    /**
     * Global EAbortCode handler
     */
    "error-const": {
      pattern: /\b(?:E\w+)\b/,
      alias: "constant",
    },

    punctuation: /|\.\.=|::|[{}[\];(),:]/,

    // abilities: {
    //   pattern: /\b(key|store|copy|drop)\b/
    // },
    // function: /\b[a-z_]\w*(?=\s*(?:::\s*<|\())/,
  };

  // Inject literals into const value;
  Prism.languages.move.const.inside.literals = Prism.languages.move.literals;

  // Prism.languages["move"]["attribute"].inside["string"] =
    // Prism.languages["move"]["string"];
})(Prism);
