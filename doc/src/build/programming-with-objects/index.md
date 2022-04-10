## Programming Objects Tutorial Series

Sui is a blockchain centered around objects. Once you start programming non-trivial smart contracts on Sui, you will start dealing with Sui objects in the code. Sui includes a very rich and comprehensive library and testing framework to allow you interact with objects in a safe and yet flexible way.

In this tutorial series, we will walk through all the powerful ways to interact with objects in Sui Move. At the end, we will also walk through the designs of a few (close-to-)real world examples to demonstrate the tradeoffs of using different object types and ownership relationships.

Prerequisite Readings:
- [Learn about Sui](../../learn/about-sui.md)
- [Sui Move](../move.md)
- [Sui Objects](../objects.md)

Index:
- [Chapter 1: Object Basics](./ch1-object-basics.md)
  - Concepts covered: Defining Move Object types, creating objects, transferring objects.
- [Chapter 2: Using Objects](./ch2-using-objects.md)
  - Concepts covered: Passing Move Objects as arguments, mutating objects, deleting objects.
- [Chapter 3: Immutable Objects](./ch3-immutable-objects.md)
  - Concepts covered: Freeze an object, using immutable objects.
- [Chapter 4: Object Wrapping](./ch4-object-wrapping.md)
  - Concepts covered: Objects wrapped in another object.
- [Chapter 5: Child Objects](./ch5-child-objects.md)
  - Concepts covered: Objects owning objects, transfer objects to objects, adding child objects, removing child objects.
- [Chapter 6: Collection and Bag](./ch6-collection-and-bag.md)
  - Concepts covered: How to use Collection and Bag library.
- [Chapter 7: Shared Objects](./ch7-shared-objects.md)
  - Concepts covered: Creating shared objects, using share objects.
- [Chapter 8: Case Study (TicTacToe)](./ch8-case-study-tictactoe.md)
  - Concepts covered: How to develop a real game with objects, trade-offs between shared objects and single-writer objects
- Chapter 9: Case Study (TBD)