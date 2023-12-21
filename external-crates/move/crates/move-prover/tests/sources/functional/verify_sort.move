// exclude_for: cvc5
module 0x42::VerifySort {
	use 0x1::vector;

	public fun verify_sort(v: &mut vector<u64>) {
		let vlen = vector::length(v);
		spec {
			assume vlen == 45;
		};
		if (vlen <= 1) return ();

		let i = 0;
		let j = 1;
		while
		({
			spec {
				invariant vlen == len(v);
				invariant i < j;
				invariant j <= vlen;
				invariant forall k in 0..i: v[k] <= v[k + 1];
				// Invariant depends on whether i was just incremented or not
				// v[i] is still in process, but previous indices are the minimum
				// elements of the vector (they've already been compared with everything)
				invariant i > 0 ==> (forall k in i..vlen: v[i-1] <= v[k]);
				// v[i] has been swapped with everything up to v[j]
				invariant forall k in i+1..j: v[i] <= v[k];
				// j stays in bounds until loop exit
				invariant i < vlen - 1 ==> j < vlen;
			};
			(i < vlen - 1)
		})
		{
			if (*vector::borrow(v, i) > *vector::borrow(v, j)) {
				vector::swap(v, i, j);
			};

			if (j < vlen - 1 ) {
				j = j + 1;
			} else {
				i = i + 1;
				j = i + 1;
			};
			// spec {
			//     TRACE(i);
			//     TRACE(j);
			//     TRACE(v);
			// }
		};
		spec {
			assert len(v) == vlen;
			assert i == vlen - 1;
			assert j == vlen;
			assert v[0] <= v[1];
			assert v[vlen - 2] <= v[vlen - 1];
		};
	}
	spec verify_sort {
		pragma verify=false; // TODO: Disabled due to timeout in CI. It verifies in a local machine.
		aborts_if false;
		ensures forall i in 0..len(v)-1: v[i] <= v[i+1];
	}

	// Assume that `v` only contains 0 or 1.
	public fun two_way_sort(v: &mut vector<u64>) {
		let _vlen = vector::length(v);
		// TODO: complete the function.
	}
	spec two_way_sort {
		// TODO: complete the spec.
	}

	// Assume that `v` only contains 0, 1 or 2.
	public fun three_way_sort(v: &mut vector<u64>) {
		let _vlen = vector::length(v);
		// TODO: complete the function.
	}
	spec three_way_sort {
		// TODO: complete the spec.
	}
}
