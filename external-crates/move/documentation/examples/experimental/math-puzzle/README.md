# Math Puzzle

This package contains an example that solves a math puzzle using Move Prover.

## Puzzle description

```
Find a,b,...,h such that:

1 <= a,b,...,h <= 9

    a b c
  +   d e
  -------
    f g h

a is the double of c
b is less than h
c is equal to e
d is equal to f
e is less than or equal to 3
f is odd
g is even
h is greater than or equal to 5
```

## Solving the puzzle using Move Prover

In `sources/puzzlie.move`, the function `Puzzle::puzzle` is constructed to take 8 numbers (i.e., `a`, `b`, ..., `h`) and abort if the input does not satisfy any rule of the puzzle. In addition, the spec block for the function is added to assert that the function always aborts, in other words, there is no input that satisfies all the rule. The Move Prover will disprove the function specification giving a counter-example which satisfies all of the puzzle rules. It will become the solution of the puzzle.

Use the following command to run Move Prover:
```
move prove
```

The following is the expected output of Move Prover:
```
    error: function does not abort under this condition
    |- /Users/jkpark/puzzle.move:53:9
    |
 36 |         aborts_if true;
    |         ^^^^^^^^^^^^^^^
    |
    =     at /Users/jkpark/puzzle.move:28: puzzle
    =         a = 6
    =         b = 5
    =         c = 3
    =         d = 7
    =         e = 3
    =         f = 7
    =         g = 2
    =         h = 6
    ...
```

## Solution

```
    a b c        6 5 3
  +   d e      +   7 3
  -------      -------
    f g h        7 2 6
```
