module a::m;

public struct X { }

fun f(cond: bool) {
   let mut _y;
   let mut _x = X { };
   if (cond) _y = X { };
   abort 0
}
