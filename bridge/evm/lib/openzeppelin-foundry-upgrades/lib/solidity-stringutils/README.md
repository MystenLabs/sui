
# String & slice utility library for Solidity
## Overview
Functionality in this library is largely implemented using an abstraction called a 'slice'. A slice represents a part of a string - anything from the entire string to a single character, or even no characters at all (a 0-length slice). Since a slice only has to specify an offset and a length, copying and manipulating slices is a lot less expensive than copying and manipulating the strings they reference.

To further reduce gas costs, most functions on slice that need to return a slice modify the original one instead of allocating a new one; for instance, `s.split(".")` will return the text up to the first '.', modifying s to only contain the remainder of the string after the '.'. In situations where you do not want to modify the original slice, you can make a copy first with `.copy()`, for example: `s.copy().split(".")`. Try and avoid using this idiom in loops; since Solidity has no memory management, it will result in allocating many short-lived slices that are later discarded.

Functions that return two slices come in two versions: a non-allocating version that takes the second slice as an argument, modifying it in place, and an allocating version that allocates and returns the second slice; see `nextRune` for example.

Functions that have to copy string data will return strings rather than slices; these can be cast back to slices for further processing if required.

## Examples
### Basic usage
    import "github.com/Arachnid/solidity-stringutils/strings.sol";

    contract Contract {
        using strings for *;

        // ...
    }

### Getting the character length of a string
    var len = "Unicode snowman ☃".toSlice().len(); // 17

### Splitting a string around a delimiter
    var s = "foo bar baz".toSlice();
    var foo = s.split(" ".toSlice());

After the above code executes, `s` is now "bar baz", and `foo` is now "foo".

### Splitting a string into an array
    var s = "www.google.com".toSlice();
    var delim = ".".toSlice();
    var parts = new string[](s.count(delim) + 1);
    for(uint i = 0; i < parts.length; i++) {
        parts[i] = s.split(delim).toString();
    }

### Extracting the middle part of a string
    var s = "www.google.com".toSlice();
    strings.slice memory part;
    s.split(".".toSlice(), part); // part and return value is "www"
    s.split(".".toSlice(), part); // part and return value is "google"

This approach uses less memory than the above, by reusing the slice `part` for each section of string extracted.

### Converting a slice back to a string
    var myString = mySlice.toString();

### Finding and returning the first occurrence of a substring
    var s = "A B C B D".toSlice();
    s.find("B".toSlice()); // "B C B D"

`find` modifies `s` to contain the part of the string from the first match onwards.

### Finding and returning the last occurrence of a substring
    var s = "A B C B D".toSlice();
    s.rfind("B".toSlice()); // "A B C B"

`rfind` modifies `s` to contain the part of the string from the last match back to the start.

### Finding without modifying the original slice.
    var s = "A B C B D".toSlice();
    var substring = s.copy().rfind("B".toSlice()); // "A B C B"

`copy` lets you cheaply duplicate a slice so you don't modify the original.

### Prefix and suffix matching
    var s = "A B C B D".toSlice();
    s.startsWith("A".toSlice()); // True
    s.endsWith("D".toSlice()); // True
    s.startsWith("B".toSlice()); // False

### Removing a prefix or suffix
    var s = "A B C B D".toSlice();
    s.beyond("A ".toSlice()).until(" D".toSlice()); // "B C B"

`beyond` modifies `s` to contain the text after its argument; `until` modifies `s` to contain the text up to its argument. If the argument isn't found, `s` is unmodified.

### Finding and returning the string up to the first match
    var s = "A B C B D".toSlice();
    var needle = "B".toSlice();
    var substring = s.until(s.copy().find(needle).beyond(needle));

Calling `find` on a copy of `s` returns the part of the string from `needle` onwards; calling `.beyond(needle)` removes `needle` as a prefix, and finally calling `s.until()` removes the entire end of the string, leaving everything up to and including the first match.

### Concatenating strings
    var s = "abc".toSlice().concat("def".toSlice()); // "abcdef"

## Reference

### toSlice(string self) internal returns (slice)
Returns a slice containing the entire string.

Arguments:

 - self The string to make a slice from.

Returns A newly allocated slice containing the entire string.
         
### copy(slice self) internal returns (slice)
Returns a new slice containing the same data as the current slice.

Arguments:

 - self The slice to copy.

Returns A new slice containing the same data as `self`.
     
### toString(slice self) internal returns (string)
    
Copies a slice to a new string.

Arguments:

 - self The slice to copy.

Returns A newly allocated string containing the slice's text.
     
### len(slice self) internal returns (uint)

Returns the length in runes of the slice. Note that this operation takes time proportional to the length of the slice; avoid using it in loops, and call `slice.empty()` if you only need to know whether the slice is empty or not.

Arguments:

 - self The slice to operate on.

Returns The length of the slice in runes.
     
### empty(slice self) internal returns (bool)
    
Returns true if the slice is empty (has a length of 0).

Arguments:

 - self The slice to operate on.

Returns True if the slice is empty, False otherwise.
     
### compare(slice self, slice other) internal returns (int)

Returns a positive number if `other` comes lexicographically after `self`, a negative number if it comes before, or zero if the contents of the two slices are equal. Comparison is done per-rune, on unicode codepoints.

Arguments:

 - self The first slice to compare.
 - other The second slice to compare.

Returns The result of the comparison.
     
### equals(slice self, slice other) internal returns (bool)
    
Returns true if the two slices contain the same text.

Arguments:

 - self The first slice to compare.
 - self The second slice to compare.

Returns True if the slices are equal, false otherwise.
     
### nextRune(slice self, slice rune) internal returns (slice)
    
Extracts the first rune in the slice into `rune`, advancing the slice to point to the next rune and returning `self`.

Arguments:

 - self The slice to operate on.
 - rune The slice that will contain the first rune.

Returns `rune`.
     
### nextRune(slice self) internal returns (slice ret)
    
Returns the first rune in the slice, advancing the slice to point to the next rune.

Arguments:

 - self The slice to operate on.

Returns A slice containing only the first rune from `self`.
     
### ord(slice self) internal returns (uint ret)
    
Returns the number of the first codepoint in the slice.

Arguments:

 - self The slice to operate on.

Returns The number of the first codepoint in the slice.
     
### keccak(slice self) internal returns (bytes32 ret)
    
Returns the keccak-256 hash of the slice.

Arguments:

 - self The slice to hash.

Returns The hash of the slice.
     
### startsWith(slice self, slice needle) internal returns (bool)

Returns true if `self` starts with `needle`.

Arguments:

 - self The slice to operate on.
 - needle The slice to search for.

Returns True if the slice starts with the provided text, false otherwise.
     
### beyond(slice self, slice needle) internal returns (slice)
    
If `self` starts with `needle`, `needle` is removed from the beginning of `self`. Otherwise, `self` is unmodified.

Arguments:

 - self The slice to operate on.
 - needle The slice to search for.

Returns `self`
     
### endsWith(slice self, slice needle) internal returns (bool)
    
Returns true if the slice ends with `needle`.

Arguments:

 - self The slice to operate on.
 - needle The slice to search for.

Returns True if the slice starts with the provided text, false otherwise.
     
### until(slice self, slice needle) internal returns (slice)
    
If `self` ends with `needle`, `needle` is removed from the end of `self`. Otherwise, `self` is unmodified.

Arguments:

 - self The slice to operate on.
 - needle The slice to search for.

Returns `self`
     
### find(slice self, slice needle) internal returns (slice)
    
Modifies `self` to contain everything from the first occurrence of `needle` to the end of the slice. `self` is set to the empty slice if `needle` is not found.

Arguments:

 - self The slice to search and modify.
 - needle The text to search for.

Returns `self`.
     
### rfind(slice self, slice needle) internal returns (slice)
    
Modifies `self` to contain the part of the string from the start of `self` to the end of the first occurrence of `needle`. If `needle` is not found, `self` is set to the empty slice.

Arguments:

 - self The slice to search and modify.
 - needle The text to search for.

Returns `self`.
     
### split(slice self, slice needle, slice token) internal returns (slice)
    
Splits the slice, setting `self` to everything after the first occurrence of `needle`, and `token` to everything before it. If `needle` does not occur in `self`, `self` is set to the empty slice, and `token` is set to the entirety of `self`.

Arguments:

 - self The slice to split.
 - needle The text to search for in `self`.
 - token An output parameter to which the first token is written.

Returns `token`.
     
### split(slice self, slice needle) internal returns (slice token)
    
Splits the slice, setting `self` to everything after the first occurrence of `needle`, and returning everything before it. If `needle` does not occur in `self`, `self` is set to the empty slice, and the entirety of `self` is returned.

Arguments:

 - self The slice to split.
 - needle The text to search for in `self`.

Returns The part of `self` up to the first occurrence of `delim`.
     
### rsplit(slice self, slice needle, slice token) internal returns (slice)
    
Splits the slice, setting `self` to everything before the last occurrence of `needle`, and `token` to everything after it. If `needle` does not occur in `self`, `self` is set to the empty slice, and `token` is set to the entirety of `self`.

Arguments:

 - self The slice to split.
 - needle The text to search for in `self`.
 - token An output parameter to which the first token is written.

Returns `token`.
     
### rsplit(slice self, slice needle) internal returns (slice token)
    
Splits the slice, setting `self` to everything before the last occurrence of `needle`, and returning everything after it. If `needle` does not occur in `self`, `self` is set to the empty slice, and the entirety of `self` is returned.

Arguments:

 - self The slice to split.
 - needle The text to search for in `self`.

Returns The part of `self` after the last occurrence of `delim`.
     
### count(slice self, slice needle) internal returns (uint count)
    
Counts the number of nonoverlapping occurrences of `needle` in `self`.

Arguments:

 - self The slice to search.
 - needle The text to search for in `self`.

Returns The number of occurrences of `needle` found in `self`.
     
### contains(slice self, slice needle) internal returns (bool)
    
Returns True if `self` contains `needle`.

Arguments:

 - self The slice to search.
 - needle The text to search for in `self`.

Returns True if `needle` is found in `self`, false otherwise.
     
### concat(slice self, slice other) internal returns (string)
    
Returns a newly allocated string containing the concatenation of `self` and `other`.

Arguments:

 - self The first slice to concatenate.
 - other The second slice to concatenate.

Returns The concatenation of the two strings.
     
### join(slice self, slice[] parts) internal returns (string)
    
Joins an array of slices, using `self` as a delimiter, returning a newly allocated string.

Arguments:

 - self The delimiter to use.
 - parts A list of slices to join.

Returns A newly allocated string containing all the slices in `parts`, joined with `self`.
