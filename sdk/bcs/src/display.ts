// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Parser 
 */

const EXPR_START = "{";
const EXPR_END = "}";
const WHITESPACE = " ";

const PATH_SEPARATOR = ".";
const FUN_PAREN_START = "(";
const FUN_PAREN_END = ")";

/** Dumb type to shorten function signatures */
type StrObj = { [key: string]: string };

/**
 * Custom implementation of Iterator<string>.
 * Enables `peek()` function to read the next value
 * without bumping the iterator.
 */
class Reader {
  /** Current position in the string */
  protected cursor: number;
  constructor(public str: string) {
    this.cursor = 0;
  }
  /** Read next char or null when finished */
  public next(): string | null {
    let el = this.peek();
    this.cursor = this.cursor + 1;
    return el;
  }
  /** Read next char without moving cursor */
  public peek(): string | null {
    let el = this.str[this.cursor];
    return (el == undefined) ? null : el;
  }
}

/**
 * Get Display for an Object by parsing the tempate strings and
 * executing expressions based on the contents of the object.
 *
 * @param template Object with string templates
 * @param suiObj Object to use as a source of data
 * @return Processed template object
 */
export function getDisplay(template: StrObj, suiObj: {}): StrObj {
  let res: StrObj = {};
  for (let i in template) {
    let { str, expressions } = parseTemplate(template[i]);
    res[i] = execute(str, expressions, suiObj);
  }

  return res;
}

/**
 * A parsed expression - only the contents of the `{...}` curly braces.
 * Later processed in the `execute` function.
 *
 * Cursor marks the position in the original string where to insert
 * the result of the expression execution.
 */
type Expression = { position: number; expr: string };

/**
 * String identifier (function name / path).
 */
type Identifier = { identifier: string; len: number };

/**
 * Parse-filter a template string and filter out the expressions.
 * Returns the resulting string and the expressions.
 */
function parseTemplate(templateStr: string): {
  str: string;
  expressions: Expression[];
} {
  let str = "";
  let iter = new Reader(templateStr);
  let char: string | null = "";
  let cursor = 0;
  let expressions = [];

  while (char !== null) {
    char = iter.next();

    // opening curly means an expression (path or function)
    // don't move the cursor on a curly!
    if (char == EXPR_START) {
      let { expr, len: _ } = parseCurly(iter);
      expressions.push({ expr, position: cursor });
    } else if (char !== null) {
      str += char;
      cursor++;
    }
  }

  return { str, expressions };
}

/**
 * Execute the `expressions` at `target` using the `source` object.
 *
 * @param target Target string to fill in expressions
 * @param expressions A set of expressions with their positions to execute
 * @param source Source object to get the data from
 * @returns Executed template with all expressions substituted
 */
function execute(
  target: string,
  expressions: Expression[],
  source: { [key: string]: any }
): string {
  // sort expressions from the end to start; so the str position
  // stays the same, even after an expression was inserted.
  expressions.sort((a, b) => b.position - a.position);

  for (let { position, expr } of expressions) {
    let parsed = parseExpression(expr);
    let value = source;

    while (parsed !== null) {
      value = value[parsed.identifier];
      parsed = parsed.next;
    }

    target = target.slice(0, position) + value + target.slice(position);
  }

  return target;
}

/**
 * Read the expression inside curly braces moving the main iterator.
 * Returns the lengh of the expression and the expression contents (ignoring whitespaces).
 */
function parseCurly(iter: Reader): { expr: string; len: number } {
  let char: string | null = "";
  let expr = "";
  let len = 0;

  while (char !== null) {
    char = iter.next();
    if (char == EXPR_END) break;
    if (char == WHITESPACE) continue;
    expr += char;
    len += 1;
  }

  return { expr: expr.trim(), len };
}

/**
 * Built-in functions. Need to be defined and processed separately.
 */
type Builtin = "df" | "hex" | "base64";

type ParsedExpression = {
  identifier: string;
  next: ParsedExpression | null;
};
// Keeping for future function extension.
// {
//   function: Builtin;
//   expression: ParsedExpression;
//   next: ParsedExpression | null;
// };

/**
 *
 */
function parseExpression(iter: string | Reader): ParsedExpression | null {
  if (typeof iter === "string") {
    iter = new Reader(iter);
  }

  let char: string | null = "";
  while (char !== null) {
    char = iter.peek();

    if (isLetter(char)) {
      let { identifier, len: _ } = parseIdent(iter);
      return {
        identifier,
        next: parseExpression(iter)
      };
    }

    iter.next();
  }

  return null;
}

/**
 * Parse alphanumeric identifier.
 * Stops at the first non-alphanum character.
 *
 * @param iter
 * @returns
 */
function parseIdent(iter: Reader): Identifier {
  let char: string | null = "";
  let identifier = "";
  let len = 0;

  while (char !== null) {
    char = iter.peek();

    if (!isLetter(char)) break;

    len++;
    iter.next();
    identifier += char;
  }

  return { identifier, len };
}

/** Check whether  */
function isLetter(char: string | null) {
  return char !== null && /[a-z0-9]/i.test(char)
}
