{# Copyright (c) The Diem Core Contributors
   SPDX-License-Identifier: Apache-2.0
#}

{# Vectors
   =======
#}

{% macro vector_module(instance) %}
{%- set S = "'" ~ instance.suffix ~ "'" -%}
{%- set T = instance.name -%}
{%- if options.native_equality -%}
{# Whole vector has native equality #}
function {:inline} $IsEqual'vec{{S}}'(v1: Vec ({{T}}), v2: Vec ({{T}})): bool {
    v1 == v2
}
{%- else -%}
// Not inlined. It appears faster this way.
function $IsEqual'vec{{S}}'(v1: Vec ({{T}}), v2: Vec ({{T}})): bool {
    LenVec(v1) == LenVec(v2) &&
    (forall i: int:: InRangeVec(v1, i) ==> $IsEqual{{S}}(ReadVec(v1, i), ReadVec(v2, i)))
}
{%- endif %}

// Not inlined.
function $IsPrefix'vec{{S}}'(v: Vec ({{T}}), prefix: Vec ({{T}})): bool {
    LenVec(v) >= LenVec(prefix) &&
    (forall i: int:: InRangeVec(prefix, i) ==> $IsEqual{{S}}(ReadVec(v, i), ReadVec(prefix, i)))
}

// Not inlined.
function $IsSuffix'vec{{S}}'(v: Vec ({{T}}), suffix: Vec ({{T}})): bool {
    LenVec(v) >= LenVec(suffix) &&
    (forall i: int:: InRangeVec(suffix, i) ==> $IsEqual{{S}}(ReadVec(v, LenVec(v) - LenVec(suffix) + i), ReadVec(suffix, i)))
}

// Not inlined.
function $IsValid'vec{{S}}'(v: Vec ({{T}})): bool {
    $IsValid'u64'(LenVec(v)) &&
    (forall i: int:: InRangeVec(v, i) ==> $IsValid{{S}}(ReadVec(v, i)))
}

{# TODO: there is an issue with existential quantifier instantiation if we use the native
   functions here without the $IsValid'u64' tag.
#}
{%- if false and instance.has_native_equality -%}
{# Vector elements have native equality #}
function {:inline} $ContainsVec{{S}}(v: Vec ({{T}}), e: {{T}}): bool {
    ContainsVec(v, e)
}

function {:inline} $IndexOfVec{{S}}(v: Vec ({{T}}), e: {{T}}): int {
    IndexOfVec(v, e)
}
{% else %}
function {:inline} $ContainsVec{{S}}(v: Vec ({{T}}), e: {{T}}): bool {
    (exists i: int :: $IsValid'u64'(i) && InRangeVec(v, i) && $IsEqual{{S}}(ReadVec(v, i), e))
}

function $IndexOfVec{{S}}(v: Vec ({{T}}), e: {{T}}): int;
axiom (forall v: Vec ({{T}}), e: {{T}}:: {$IndexOfVec{{S}}(v, e)}
    (var i := $IndexOfVec{{S}}(v, e);
     if (!$ContainsVec{{S}}(v, e)) then i == -1
     else $IsValid'u64'(i) && InRangeVec(v, i) && $IsEqual{{S}}(ReadVec(v, i), e) &&
        (forall j: int :: $IsValid'u64'(j) && j >= 0 && j < i ==> !$IsEqual{{S}}(ReadVec(v, j), e))));
{% endif %}

function {:inline} $RangeVec{{S}}(v: Vec ({{T}})): $Range {
    $Range(0, LenVec(v))
}


function {:inline} $EmptyVec{{S}}(): Vec ({{T}}) {
    EmptyVec()
}

procedure {:inline 1} $1_vector_empty{{S}}() returns (v: Vec ({{T}})) {
    v := EmptyVec();
}

function {:inline} $1_vector_$empty{{S}}(): Vec ({{T}}) {
    EmptyVec()
}

procedure {:inline 1} $1_vector_is_empty{{S}}(v: Vec ({{T}})) returns (b: bool) {
    b := IsEmptyVec(v);
}

procedure {:inline 1} $1_vector_push_back{{S}}(m: $Mutation (Vec ({{T}})), val: {{T}}) returns (m': $Mutation (Vec ({{T}}))) {
    m' := $UpdateMutation(m, ExtendVec($Dereference(m), val));
}

function {:inline} $1_vector_$push_back{{S}}(v: Vec ({{T}}), val: {{T}}): Vec ({{T}}) {
    ExtendVec(v, val)
}

procedure {:inline 1} $1_vector_pop_back{{S}}(m: $Mutation (Vec ({{T}}))) returns (e: {{T}}, m': $Mutation (Vec ({{T}}))) {
    var v: Vec ({{T}});
    var len: int;
    v := $Dereference(m);
    len := LenVec(v);
    if (len == 0) {
        call $ExecFailureAbort();
        return;
    }
    e := ReadVec(v, len-1);
    m' := $UpdateMutation(m, RemoveVec(v));
}

procedure {:inline 1} $1_vector_append{{S}}(m: $Mutation (Vec ({{T}})), other: Vec ({{T}})) returns (m': $Mutation (Vec ({{T}}))) {
    m' := $UpdateMutation(m, ConcatVec($Dereference(m), other));
}

procedure {:inline 1} $1_vector_reverse{{S}}(m: $Mutation (Vec ({{T}}))) returns (m': $Mutation (Vec ({{T}}))) {
    m' := $UpdateMutation(m, ReverseVec($Dereference(m)));
}

procedure {:inline 1} $1_vector_reverse_append{{S}}(m: $Mutation (Vec ({{T}})), other: Vec ({{T}})) returns (m': $Mutation (Vec ({{T}}))) {
    m' := $UpdateMutation(m, ConcatVec($Dereference(m), ReverseVec(other)));
}

procedure {:inline 1} $1_vector_trim_reverse{{S}}(m: $Mutation (Vec ({{T}})), new_len: int) returns (v: (Vec ({{T}})), m': $Mutation (Vec ({{T}}))) {
    var len: int;
    v := $Dereference(m);
    if (LenVec(v) < new_len) {
        call $ExecFailureAbort();
        return;
    }
    v := SliceVec(v, new_len, LenVec(v));
    v := ReverseVec(v);
    m' := $UpdateMutation(m, SliceVec($Dereference(m), 0, new_len));
}

procedure {:inline 1} $1_vector_trim{{S}}(m: $Mutation (Vec ({{T}})), new_len: int) returns (v: (Vec ({{T}})), m': $Mutation (Vec ({{T}}))) {
    var len: int;
    v := $Dereference(m);
    if (LenVec(v) < new_len) {
        call $ExecFailureAbort();
        return;
    }
    v := SliceVec(v, new_len, LenVec(v));
    m' := $UpdateMutation(m, SliceVec($Dereference(m), 0, new_len));
}

procedure {:inline 1} $1_vector_reverse_slice{{S}}(m: $Mutation (Vec ({{T}})), left: int, right: int) returns (m': $Mutation (Vec ({{T}}))) {
    var left_vec: Vec ({{T}});
    var mid_vec: Vec ({{T}});
    var right_vec: Vec ({{T}});
    var v: Vec ({{T}});
    if (left > right) {
        call $ExecFailureAbort();
        return;
    }
    if (left == right) {
        m' := m;
        return;
    }
    v := $Dereference(m);
    if (!(right >= 0 && right <= LenVec(v))) {
        call $ExecFailureAbort();
        return;
    }
    left_vec := SliceVec(v, 0, left);
    right_vec := SliceVec(v, right, LenVec(v));
    mid_vec := ReverseVec(SliceVec(v, left, right));
    m' := $UpdateMutation(m, ConcatVec(left_vec, ConcatVec(mid_vec, right_vec)));
}

procedure {:inline 1} $1_vector_rotate{{S}}(m: $Mutation (Vec ({{T}})), rot: int) returns (n: int, m': $Mutation (Vec ({{T}}))) {
    var v: Vec ({{T}});
    var len: int;
    var left_vec: Vec ({{T}});
    var right_vec: Vec ({{T}});
    v := $Dereference(m);
    if (!(rot >= 0 && rot <= LenVec(v))) {
        call $ExecFailureAbort();
        return;
    }
    left_vec := SliceVec(v, 0, rot);
    right_vec := SliceVec(v, rot, LenVec(v));
    m' := $UpdateMutation(m, ConcatVec(right_vec, left_vec));
    n := LenVec(v) - rot;
}

procedure {:inline 1} $1_vector_rotate_slice{{S}}(m: $Mutation (Vec ({{T}})), left: int, rot: int, right: int) returns (n: int, m': $Mutation (Vec ({{T}}))) {
    var left_vec: Vec ({{T}});
    var mid_vec: Vec ({{T}});
    var right_vec: Vec ({{T}});
    var mid_left_vec: Vec ({{T}});
    var mid_right_vec: Vec ({{T}});
    var v: Vec ({{T}});
    v := $Dereference(m);
    if (!(left <= rot && rot <= right)) {
        call $ExecFailureAbort();
        return;
    }
    if (!(right >= 0 && right <= LenVec(v))) {
        call $ExecFailureAbort();
        return;
    }
    v := $Dereference(m);
    left_vec := SliceVec(v, 0, left);
    right_vec := SliceVec(v, right, LenVec(v));
    mid_left_vec := SliceVec(v, left, rot);
    mid_right_vec := SliceVec(v, rot, right);
    mid_vec := ConcatVec(mid_right_vec, mid_left_vec);
    m' := $UpdateMutation(m, ConcatVec(left_vec, ConcatVec(mid_vec, right_vec)));
    n := left + (right - rot);
}

procedure {:inline 1} $1_vector_insert{{S}}(m: $Mutation (Vec ({{T}})), i: int, e: {{T}}) returns (m': $Mutation (Vec ({{T}}))) {
    var left_vec: Vec ({{T}});
    var right_vec: Vec ({{T}});
    var v: Vec ({{T}});
    v := $Dereference(m);
    if (!(i >= 0 && i <= LenVec(v))) {
        call $ExecFailureAbort();
        return;
    }
    if (i == LenVec(v)) {
        m' := $UpdateMutation(m, ExtendVec(v, e));
    } else {
        left_vec := ExtendVec(SliceVec(v, 0, i), e);
        right_vec := SliceVec(v, i, LenVec(v));
        m' := $UpdateMutation(m, ConcatVec(left_vec, right_vec));
    }
}

procedure {:inline 1} $1_vector_length{{S}}(v: Vec ({{T}})) returns (l: int) {
    l := LenVec(v);
}

function {:inline} $1_vector_$length{{S}}(v: Vec ({{T}})): int {
    LenVec(v)
}

procedure {:inline 1} $1_vector_borrow{{S}}(v: Vec ({{T}}), i: int) returns (dst: {{T}}) {
    if (!InRangeVec(v, i)) {
        call $ExecFailureAbort();
        return;
    }
    dst := ReadVec(v, i);
}

function {:inline} $1_vector_$borrow{{S}}(v: Vec ({{T}}), i: int): {{T}} {
    ReadVec(v, i)
}

procedure {:inline 1} $1_vector_borrow_mut{{S}}(m: $Mutation (Vec ({{T}})), index: int)
returns (dst: $Mutation ({{T}}), m': $Mutation (Vec ({{T}})))
{
    var v: Vec ({{T}});
    v := $Dereference(m);
    if (!InRangeVec(v, index)) {
        call $ExecFailureAbort();
        return;
    }
    dst := $Mutation(m->l, ExtendVec(m->p, index), ReadVec(v, index));
    m' := m;
}

function {:inline} $1_vector_$borrow_mut{{S}}(v: Vec ({{T}}), i: int): {{T}} {
    ReadVec(v, i)
}

procedure {:inline 1} $1_vector_destroy_empty{{S}}(v: Vec ({{T}})) {
    if (!IsEmptyVec(v)) {
      call $ExecFailureAbort();
    }
}

procedure {:inline 1} $1_vector_swap{{S}}(m: $Mutation (Vec ({{T}})), i: int, j: int) returns (m': $Mutation (Vec ({{T}})))
{
    var v: Vec ({{T}});
    v := $Dereference(m);
    if (!InRangeVec(v, i) || !InRangeVec(v, j)) {
        call $ExecFailureAbort();
        return;
    }
    m' := $UpdateMutation(m, SwapVec(v, i, j));
}

function {:inline} $1_vector_$swap{{S}}(v: Vec ({{T}}), i: int, j: int): Vec ({{T}}) {
    SwapVec(v, i, j)
}

procedure {:inline 1} $1_vector_remove{{S}}(m: $Mutation (Vec ({{T}})), i: int) returns (e: {{T}}, m': $Mutation (Vec ({{T}})))
{
    var v: Vec ({{T}});

    v := $Dereference(m);

    if (!InRangeVec(v, i)) {
        call $ExecFailureAbort();
        return;
    }
    e := ReadVec(v, i);
    m' := $UpdateMutation(m, RemoveAtVec(v, i));
}

procedure {:inline 1} $1_vector_swap_remove{{S}}(m: $Mutation (Vec ({{T}})), i: int) returns (e: {{T}}, m': $Mutation (Vec ({{T}})))
{
    var len: int;
    var v: Vec ({{T}});

    v := $Dereference(m);
    len := LenVec(v);
    if (!InRangeVec(v, i)) {
        call $ExecFailureAbort();
        return;
    }
    e := ReadVec(v, i);
    m' := $UpdateMutation(m, RemoveVec(SwapVec(v, i, len-1)));
}

procedure {:inline 1} $1_vector_contains{{S}}(v: Vec ({{T}}), e: {{T}}) returns (res: bool)  {
    res := $ContainsVec{{S}}(v, e);
}

procedure {:inline 1}
$1_vector_index_of{{S}}(v: Vec ({{T}}), e: {{T}}) returns (res1: bool, res2: int) {
    res2 := $IndexOfVec{{S}}(v, e);
    if (res2 >= 0) {
        res1 := true;
    } else {
        res1 := false;
        res2 := 0;
    }
}
{% endmacro vector_module %}

{# Tables
   =======
#}

{% macro table_key_encoding(instance) %}
{%- set K = instance.name -%}
{%- set S = "'" ~ instance.suffix ~ "'" -%}

function $EncodeKey{{S}}(k: {{K}}): int;
axiom (
  forall k1, k2: {{K}} :: {$EncodeKey{{S}}(k1), $EncodeKey{{S}}(k2)}
    $IsEqual{{S}}(k1, k2) <==> $EncodeKey{{S}}(k1) == $EncodeKey{{S}}(k2)
);
{% endmacro table_key_encoding %}


{% macro table_module(impl, instance) %}
{%- set K = instance.0.name -%}
{%- set V = instance.1.name -%}
{%- set Type = impl.struct_name -%}
{%- set Self = "Table int (" ~ V ~ ")" -%}
{%- set S = "'" ~ instance.0.suffix ~ "_" ~ instance.1.suffix ~ "'" -%}
{%- set SV = "'" ~ instance.1.suffix ~ "'" -%}
{%- set ENC = "$EncodeKey'" ~ instance.0.suffix ~ "'" -%}

{%- if options.native_equality -%}
function $IsEqual'{{Type}}{{S}}'(t1: {{Self}}, t2: {{Self}}): bool {
    t1 == t2
}
{%- else -%}
function $IsEqual'{{Type}}{{S}}'(t1: {{Self}}, t2: {{Self}}): bool {
    LenTable(t1) == LenTable(t2) &&
    (forall k: int :: ContainsTable(t1, k) <==> ContainsTable(t2, k)) &&
    (forall k: int :: ContainsTable(t1, k) ==> GetTable(t1, k) == GetTable(t2, k)) &&
    (forall k: int :: ContainsTable(t2, k) ==> GetTable(t1, k) == GetTable(t2, k))
}
{%- endif %}

// Not inlined.
function $IsValid'{{Type}}{{S}}'(t: {{Self}}): bool {
    $IsValid'u64'(LenTable(t)) &&
    (forall i: int:: ContainsTable(t, i) ==> $IsValid{{SV}}(GetTable(t, i)))
}

{%- if impl.fun_new != "" %}
procedure {:inline 2} {{impl.fun_new}}{{S}}() returns (v: {{Self}}) {
    v := EmptyTable();
}
{%- endif %}

{%- if impl.fun_destroy_empty != "" %}
procedure {:inline 2} {{impl.fun_destroy_empty}}{{S}}(t: {{Self}}) {
    if (LenTable(t) != 0) {
        call $Abort($StdError(1/*INVALID_STATE*/, 102/*ENOT_EMPTY*/));
    }
}
{%- endif %}

{%- if impl.fun_len != "" %}
procedure {:inline 2} {{impl.fun_len}}{{S}}(t: ({{Self}})) returns (l: int) {
    l := LenTable(t);
}
{%- endif %}

{%- if impl.fun_is_empty != "" %}
procedure {:inline 2} {{impl.fun_is_empty}}{{S}}(t: ({{Self}})) returns (r: bool) {
    r := LenTable(t) == 0;
}
{%- endif %}

{%- if impl.fun_has_key != "" %}
procedure {:inline 2} {{impl.fun_has_key}}{{S}}(t: ({{Self}}), k: {{K}}) returns (r: bool) {
    r := ContainsTable(t, {{ENC}}(k));
}
{%- endif %}

{%- if impl.fun_add_no_override != "" %}
procedure {:inline 2} {{impl.fun_add_no_override}}{{S}}(m: $Mutation ({{Self}}), k: {{K}}, v: {{V}}) returns (m': $Mutation({{Self}})) {
    var enc_k: int;
    var t: {{Self}};
    enc_k := {{ENC}}(k);
    t := $Dereference(m);
    if (ContainsTable(t, enc_k)) {
        call $Abort($StdError(7/*INVALID_ARGUMENTS*/, 100/*EALREADY_EXISTS*/));
    } else {
        m' := $UpdateMutation(m, AddTable(t, enc_k, v));
    }
}
{%- endif %}

{%- if impl.fun_add_override_if_exists != "" %}
procedure {:inline 2} {{impl.fun_add_override_if_exists}}{{S}}(m: $Mutation ({{Self}}), k: {{K}}, v: {{V}}) returns (m': $Mutation({{Self}})) {
    var enc_k: int;
    var t: {{Self}};
    enc_k := {{ENC}}(k);
    t := $Dereference(m);
    if (ContainsTable(t, enc_k)) {
        m' := $UpdateMutation(m, UpdateTable(t, enc_k, v));
    } else {
        m' := $UpdateMutation(m, AddTable(t, enc_k, v));
    }
}
{%- endif %}

{%- if impl.fun_del_must_exist != "" %}
procedure {:inline 2} {{impl.fun_del_must_exist}}{{S}}(m: $Mutation ({{Self}}), k: {{K}})
returns (v: {{V}}, m': $Mutation({{Self}})) {
    var enc_k: int;
    var t: {{Self}};
    enc_k := {{ENC}}(k);
    t := $Dereference(m);
    if (!ContainsTable(t, enc_k)) {
        call $Abort($StdError(7/*INVALID_ARGUMENTS*/, 101/*ENOT_FOUND*/));
    } else {
        v := GetTable(t, enc_k);
        m' := $UpdateMutation(m, RemoveTable(t, enc_k));
    }
}
{%- endif %}

{%- if impl.fun_del_return_key != "" %}
procedure {:inline 2} {{impl.fun_del_return_key}}{{S}}(m: $Mutation ({{Self}}), k: {{K}})
returns (k': {{K}}, v: {{V}}, m': $Mutation({{Self}})) {
    var enc_k: int;
    var t: {{Self}};
    enc_k := {{ENC}}(k);
    t := $Dereference(m);
    if (!ContainsTable(t, enc_k)) {
        call $Abort($StdError(7/*INVALID_ARGUMENTS*/, 101/*ENOT_FOUND*/));
    } else {
        k' := k;
        v := GetTable(t, enc_k);
        m' := $UpdateMutation(m, RemoveTable(t, enc_k));
    }
}
{%- endif %}

{%- if impl.fun_borrow != "" %}
procedure {:inline 2} {{impl.fun_borrow}}{{S}}(t: {{Self}}, k: {{K}}) returns (v: {{V}}) {
    var enc_k: int;
    enc_k := {{ENC}}(k);
    if (!ContainsTable(t, enc_k)) {
        call $Abort($StdError(7/*INVALID_ARGUMENTS*/, 101/*ENOT_FOUND*/));
    } else {
        v := GetTable(t, {{ENC}}(k));
    }
}
{%- endif %}

{%- if impl.fun_borrow_mut != "" %}
procedure {:inline 2} {{impl.fun_borrow_mut}}{{S}}(m: $Mutation ({{Self}}), k: {{K}})
returns (dst: $Mutation ({{V}}), m': $Mutation ({{Self}})) {
    var enc_k: int;
    var t: {{Self}};
    enc_k := {{ENC}}(k);
    t := $Dereference(m);
    if (!ContainsTable(t, enc_k)) {
        call $Abort($StdError(7/*INVALID_ARGUMENTS*/, 101/*ENOT_FOUND*/));
    } else {
        dst := $Mutation(m->l, ExtendVec(m->p, enc_k), GetTable(t, enc_k));
        m' := m;
    }
}
{%- endif %}

{%- if impl.fun_borrow_mut_with_default != "" %}
procedure {:inline 2} {{impl.fun_borrow_mut_with_default}}{{S}}(m: $Mutation ({{Self}}), k: {{K}}, default: {{V}})
returns (dst: $Mutation ({{V}}), m': $Mutation ({{Self}})) {
    var enc_k: int;
    var t: {{Self}};
    var t': {{Self}};
    enc_k := {{ENC}}(k);
    t := $Dereference(m);
    if (!ContainsTable(t, enc_k)) {
        m' := $UpdateMutation(m, AddTable(t, enc_k, default));
        t' := $Dereference(m');
        dst := $Mutation(m'->l, ExtendVec(m'->p, enc_k), GetTable(t', enc_k));
    } else {
        dst := $Mutation(m->l, ExtendVec(m->p, enc_k), GetTable(t, enc_k));
        m' := m;
    }
}
{%- endif %}

{%- if impl.fun_spec_len != "" %}
function {:inline} {{impl.fun_spec_len}}{{S}}(t: ({{Self}})): int {
    LenTable(t)
}
{%- endif %}

{%- if impl.fun_spec_is_empty != "" %}
function {:inline} {{impl.fun_spec_is_empty}}{{S}}(t: ({{Self}})): bool {
    LenTable(t) == 0
}
{%- endif %}

{%- if impl.fun_spec_has_key != "" %}
function {:inline} {{impl.fun_spec_has_key}}{{S}}(t: ({{Self}}), k: {{K}}): bool {
    ContainsTable(t, {{ENC}}(k))
}
{%- endif %}

{%- if impl.fun_spec_set != "" %}
function {:inline} {{impl.fun_spec_set}}{{S}}(t: {{Self}}, k: {{K}}, v: {{V}}): {{Self}} {
    (var enc_k := {{ENC}}(k);
    if (ContainsTable(t, enc_k)) then
        UpdateTable(t, enc_k, v)
    else
        AddTable(t, enc_k, v))
}
{%- endif %}

{%- if impl.fun_spec_del != "" %}
function {:inline} {{impl.fun_spec_del}}{{S}}(t: {{Self}}, k: {{K}}): {{Self}} {
    RemoveTable(t, {{ENC}}(k))
}
{%- endif %}

{%- if impl.fun_spec_get != "" %}
function {:inline} {{impl.fun_spec_get}}{{S}}(t: {{Self}}, k: {{K}}): {{V}} {
    GetTable(t, {{ENC}}(k))
}
{%- endif %}

{%- if impl.fun_spec_new != "" %}
function {:inline} {{impl.fun_spec_new}}{{S}}(): {{Self}} {
    EmptyTable()
}
{%- endif %}

{% endmacro table_module %}


{# BCS
   ====
#}

{% macro bcs_module(instance) %}
{%- set S = "'" ~ instance.suffix ~ "'" -%}
{%- set T = instance.name -%}
// Serialize is modeled as an uninterpreted function, with an additional
// axiom to say it's an injection.

function $1_bcs_serialize{{S}}(v: {{T}}): Vec int;

axiom (forall v1, v2: {{T}} :: {$1_bcs_serialize{{S}}(v1), $1_bcs_serialize{{S}}(v2)}
   $IsEqual{{S}}(v1, v2) <==> $IsEqual'vec'u8''($1_bcs_serialize{{S}}(v1), $1_bcs_serialize{{S}}(v2)));

// This says that serialize returns a non-empty vec<u8>
{% if options.serialize_bound == 0 %}
axiom (forall v: {{T}} :: {$1_bcs_serialize{{S}}(v)}
     ( var r := $1_bcs_serialize{{S}}(v); $IsValid'vec'u8''(r) && LenVec(r) > 0 ));
{% else %}
axiom (forall v: {{T}} :: {$1_bcs_serialize{{S}}(v)}
     ( var r := $1_bcs_serialize{{S}}(v); $IsValid'vec'u8''(r) && LenVec(r) > 0 &&
                            LenVec(r) <= {{options.serialize_bound}} ));
{% endif %}

procedure $1_bcs_to_bytes{{S}}(v: {{T}}) returns (res: Vec int);
ensures res == $1_bcs_serialize{{S}}(v);

function {:inline} $1_bcs_$to_bytes{{S}}(v: {{T}}): Vec int {
    $1_bcs_serialize{{S}}(v)
}

{% if S == "'address'" -%}
// Serialized addresses should have the same length.
const $serialized_address_len: int;
// Serialized addresses should have the same length
axiom (forall v: int :: {$1_bcs_serialize'address'(v)}
     ( var r := $1_bcs_serialize'address'(v); LenVec(r) == $serialized_address_len));
{% endif %}
{% endmacro hash_module %}


{# Event Module
   ============
#}

{% macro event_module(instance) %}
{%- set S = "'" ~ instance.suffix ~ "'" -%}
{%- set T = instance.name -%}

// Map type specific handle to universal one.
type $1_event_EventHandle{{S}} = $1_event_EventHandle;

function {:inline} $IsEqual'$1_event_EventHandle{{S}}'(a: $1_event_EventHandle{{S}}, b: $1_event_EventHandle{{S}}): bool {
    a == b
}

function $IsValid'$1_event_EventHandle{{S}}'(h: $1_event_EventHandle{{S}}): bool {
    true
}

// Embed event `{{T}}` into universal $EventRep
function {:constructor} $ToEventRep{{S}}(e: {{T}}): $EventRep;
axiom (forall v1, v2: {{T}} :: {$ToEventRep{{S}}(v1), $ToEventRep{{S}}(v2)}
    $IsEqual{{S}}(v1, v2) <==> $ToEventRep{{S}}(v1) == $ToEventRep{{S}}(v2));

// Creates a new event handle. This ensures each time it is called that a unique new abstract event handler is
// returned.
// TODO: we should check (and abort with the right code) if no generator exists for the signer.
procedure {:inline 1} $1_event_new_event_handle{{S}}(signer: $signer) returns (res: $1_event_EventHandle{{S}}) {
    assume $1_event_EventHandles[res] == false;
    $1_event_EventHandles := $1_event_EventHandles[res := true];
}

// This boogie procedure is the model of `emit_event`. This model abstracts away the `counter` behavior, thus not
// mutating (or increasing) `counter`.
procedure {:inline 1} $1_event_emit_event{{S}}(handle_mut: $Mutation $1_event_EventHandle{{S}}, msg: {{T}})
returns (res: $Mutation $1_event_EventHandle{{S}}) {
    var handle: $1_event_EventHandle{{S}};
    handle := $Dereference(handle_mut);
    $es := $ExtendEventStore{{S}}($es, handle, msg);
    res := handle_mut;
}

procedure {:inline 1} $1_event_guid{{S}}(handle_ref: $1_event_EventHandle{{S}})
returns (res: int) {
    // TODO: temporarily mocked. The return type needs to be fixed.
    res := 0;
}

procedure {:inline 1} $1_event_counter{{S}}(handle_ref: $1_event_EventHandle{{S}})
returns (res: int) {
    // TODO: temporarily mocked.
    res := 0;
}

procedure {:inline 1} $1_event_destroy_handle{{S}}(handle: $1_event_EventHandle{{S}}) {
}

function {:inline} $ExtendEventStore{{S}}(
        es: $EventStore, handle: $1_event_EventHandle{{S}}, msg: {{T}}): $EventStore {
    (var stream := es->streams[handle];
    (var stream_new := ExtendMultiset(stream, $ToEventRep{{S}}(msg));
    $EventStore(es->counter+1, es->streams[handle := stream_new])))
}

function {:inline} $CondExtendEventStore{{S}}(
        es: $EventStore, handle: $1_event_EventHandle{{S}}, msg: {{T}}, cond: bool): $EventStore {
    if cond then
        $ExtendEventStore{{S}}(es, handle, msg)
    else
        es
}
{% endmacro event_module %}
