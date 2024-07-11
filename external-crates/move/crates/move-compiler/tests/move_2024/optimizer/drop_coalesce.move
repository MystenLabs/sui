module a::m;

public struct X { }

fun f(cond: bool) {
   let mut _y;
   let mut _x = X { };
   if (cond) {
      let X { } = _x;
   } else {
      let X { } = _x;
   };
   _y = X { };
   if (cond) {
      let X { } = _y;
   } else {
      let X { } = _y;
   };
}
