Tests in this directory are intended to exercise the merging of dependencies / fields between the `[dependencies]`
and the `[dep-replacements]` sections of the manifest.

Possible fields
======

 - git, rev, path
 - local
 - on-chain
 - r,

 - override
 - rename-from

 - published-at
 - use-environment

Scenarios
=========

## deps: local = "foo/"; replacements: None
expected: local = "foo/"

## deps: None; replacements: local = "foo/"
expected: local = "foo/"

## deps: {}
expected: "invalid dependency; expected one of `git`, `local`, `on-chain`, or `r`

## deps: {local = "A/", on-chain = true}
expected: "invalid dependency; expected one of `git`, `local`, `on-chain`, or `r`

## (?) deps: git = "A.git" rev = "0x1"; replacements: rev = "0x2"
? expected: git = "A.git", rev = "0x2"
? expected error at replacements.rev
  unexpected field `rev`

## (?) deps: git = "A.git"; replacements: rev = "0x2"
? expected: git = "A.git", rev = "0x2"
? expected error at replacements.rev:
  unexpected field `rev`

## (?) deps: git = "A.git" path = "A/"; replacements: path = "B/"
expected: git = "A.git", path = "B/"

## (?) deps: git = "A.git"; replacements: rev = "0x2"
expected: git = "A.git", rev = "0x2"

## deps: git = "A.git" rev = "0x1"; replacements: git = "B.git"
expected: git = "B.git", rev = None

## deps: local = "foo/"; replacements: rev = "0x2"
expected (error at replacements.rev): "rev can only be used with `git` dependencies"

## deps: git = "A.git"; replacements: local = "P/", rev = "0x2"
expected (error at replacements.rev): "rev can only be used with `git` dependencies"

## deps: git = "A.git"; replacements: use-environment = "mainnet"
expected: git = "A.git", env = "mainnet"

## deps: git = "A.git", use-environment = "mainnet"
expected (deps.use-environment):
  `use-environment` can only be used in `[dep-replacements]`. Consider replacing with
    [dep-replacements.mainnet]
    foo = { use-environment = "mainnet" }

## deps: git = "A.git", published-at: "0x1234"
expected (error on deps.published-at):
 `published-at` can only be used in `[dep-replacements]`. Consider replacing with
   [dep-replacements.mainnet]
   foo = { published-at = "0x1234" }

## deps: local = "A/", override = true; replacements: override = false
? expected (error on replacements.override):

## deps: local = "A/"; replacements: override = true
? expected (error on replacements.override)

## deps: local = "A/"; replacements: rename-from = "A"
? expected error on replacements.rename-from

## deps: local = "A/" rename-from = "A"; replacements: local = "B/"
expected: local = "B/", rename-from = None

## deps: local = "A/", rename-from = "A"; replacements: use-environment = "mainnet"
expected: local = "A/", rename-from: "A", use-environment: "mainnet"
