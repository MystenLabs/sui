# Sui documentation voice

House voice for docs.sui.io and other Mysten Labs documentation.

**This file is subordinate to the style guide.** `docs/sui-documentation-style-guide.skill` owns every mechanical rule: punctuation, capitalization, terminology, admonitions, headings, lists, links, person, tense, and voice. Nothing here repeats or overrides it. Where the two ever appear to disagree, the style guide wins and this file is wrong.

What is left is the part a rule list cannot capture: how an explanation is built, and how the result should sound.

## How a page should sound

Informational without being stiff. Documentation is not trying to be entertaining, and it is not a legal document either.

Three registers to stay out of:

- **Corporate.** No "leverage", "utilize", "solution", "empower", "seamless", "unlock".
- **Bloggy.** No "So here's the thing", "Let's dive in", "spoiler alert". No "let's" at all: address the reader directly.
- **Academic.** No "it should be noted that", "one might argue", "the aforementioned".

State facts and let them carry the weight. "Query calls return in 200 to 400 ms" beats "query calls are designed to be extremely fast". Confidence comes from precision, not from adjectives.

## How an explanation is built

**Define before you explain.** Almost every page and section should open by saying what the subject *is* before what it does or how to use it. The shape is `X is a Y that Z`, then why it matters, then how to use it.

> An `AccountWrapper` is a shared object that holds a trader's account and mediates access to it.

A page that opens with usage instructions for a thing it never defined has skipped the reader's first question.

**Compare to the familiar.** Bridge a new concept to one the reader already has. Keep the comparison concrete and literal, and drop it once it has done its work. Analogies drawn from consumer finance, sports, driving, or anything culturally specific fail for a global audience.

**Categorize before you enumerate.** When something has parts, say how many and what kind, then take them in order. A reader who knows a list has four items reads it differently from one who does not.

**Build simple to complex.** Define, then explain the mechanism, then state the implications. Do not open at the edge case.

**Earn every term.** A term is either defined where it first appears or is one the audience is assumed to know. There is no third category. Introducing a term and explaining it two sections later is the most common version of this failure.

## Code in a page

**Introduce every code block.** A sentence ending in a colon, saying what the block shows. Never drop a bare fence into the page.

**Explanation precedes code, not the other way round.** A reader who does not yet know what the code is for cannot read it.

**Break down what is worth breaking down.** After a block doing something non-obvious, walk the steps in order. Skip this for a block that speaks for itself; a numbered restatement of three self-evident lines is noise.

**Conceptual sections come before procedural ones.** Explain the model, then the steps.

## Cross-references

Standard phrasing is **"Learn more about ..."**, in preference to "see also", "refer to", or "check out".

Never link without saying what is on the other side. A bare link, or one whose text is the URL, makes the reader click to find out whether they needed to.

## What not to do

- **No filler openers.** Not "In today's world", not "As we all know".
- **No throat-clearing.** The first sentence carries information.
- **No summary paragraph** restating the section that just ended.
- **No hand-waving.** If a mechanism matters, explain it. If it does not, cut it.
- **No promotional language** anywhere in technical content. Not "revolutionary", "cutting-edge", "best-in-class", "unparalleled".
- **No editorializing.** Documentation says what a thing is and how it works. Opinions belong in a blog post.
- **No walls of text.** Break with headings, lists, and code.
- **Do not bury the answer.** A reader arrives with a question. The answer should be visible without scrolling.
- **Do not over-explain** what the page's own audience already knows.

## Using this file in a review

Findings drawn from this file are **suggestions, never blocking**. Voice is a preference and a matter of degree; the style guide is the rule. A page that reads a little flat but breaks nothing is not defective.

Two exceptions worth raising with more confidence, because they cost the reader real time: a section that uses a term it never defined, and a page that buries its answer below the fold.
