Each test case should include:

- all possible expressions inside every block
- line and block comments in both trailing and leading positions
- for lists and blocks, both break and non-break lines
- trailing comma checks for any types of lists
- chained expressions
- binary expressions
- expression inside control flow statements (breaking and non-breaking)


Scenario complicado:

1. List
2. Block
3. Lambda
4. Chain (chain + lambda)
5. Binary

What about groups? When do we use them and how do we use them gently? How to not
break the parent group with a breaking child?

//

// if lists should not be groups, then what do we expect when we want to break
// them? that's a little or - very much - confusing
