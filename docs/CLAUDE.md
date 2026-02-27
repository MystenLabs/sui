# Documentation Guidelines

## Style Checking

After creating or modifying MDX documentation files, run the `/copy-edit-docs` skill to check for style violations:

```
/copy-edit-docs <file-or-directory>
```

This checks for:
- License headers in MDX files (should not have them)
- Sentence case for headings
- Periods at end of list items
- Backticks for code references in link text
- Sidebar entry requirements
- Frontmatter requirements (title, description, keywords)
