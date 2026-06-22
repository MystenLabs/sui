# sui prompt — expert Sui and Move knowledge for AI agents

`sui prompt` prints expert Sui and Move knowledge from embedded skill bundles,
organized into **categories**.

## How to use this

Read the available categories (`sui prompt categories`), try to match one to the
task, then drill into its bundles. Each skill is a two-tier bundle: `SKILL.md`
routes, reference files hold content. **Read every reference file** before
applying — `--all` loads them in one call.

```sh
sui prompt categories                    # see the available categories
sui prompt category <name> --all         # read every bundle's content in one call
sui prompt category <name>               # read a category's workflow + skill list
sui prompt category <name> --list        # list bundle and reference file names and sizes (no content)
```

Skills can also be reached directly:

```sh
sui prompt skills                        # list all skill bundles, flat
sui prompt skill <bundle> --all          # read SKILL.md + every reference file
sui prompt skill <bundle>                # read a bundle's SKILL.md
sui prompt skill <bundle> --list         # list reference file names and sizes (no content)
sui prompt skill <bundle> --file <ref>   # read a specific reference file
```