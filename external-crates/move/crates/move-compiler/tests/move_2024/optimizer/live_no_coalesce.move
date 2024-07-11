module a::m;

public struct X { }

fun f(cond: bool) {
   let mut _x = X { };
   let mut _y;
   if (cond) {
      _y = _x;
   } else {
      let X { } = _x;
      _y = X { };
   };
   let X { } = _y;
   _y = X { };
   if (cond) {
      let X { } = _y;
   } else {
      let X { } = _y;
   };
}
