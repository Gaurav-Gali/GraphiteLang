# LLD 11 — Numeric operator promotion

Operator compatibility is separate from the Stage 5 conversion registry.
`auto T` and explicit `T(value)` keep their existing rules. An arithmetic
expression's type comes from the operands, not from the variable that stores
it. `flt sum = a + b` does not make `a + b` a float when `a` and `b` are
integers.

## Families and widths

Canonical types only. Aliases are not extra types: `int` is `int64`, `uint`
is `uint64`, and `flt` is `flt64`.

Family precedence for a mixed arithmetic result is:

**flt > int > uint**

Within the chosen family, the result is the narrowest width that can hold
every value of both operand types. If no width can, the result is the widest
width in that family and each operand that might not fit is checked at
runtime.

| Operands | Result |
|---|---|
| `int16 + int16` | `int16` |
| `int16 + int32` | `int32` |
| `int16 + int64` | `int64` |
| `int16 + uint16` | `int32` |
| `int16 + uint32` | `int64` |
| `int16 + uint64` | `int64`, checking the `uint64` |
| `int32 + uint16` | `int32` |
| `int32 + uint32` | `int64` |
| `int32 + uint64` | `int64`, checking the `uint64` |
| `int64 + uint16` | `int64` |
| `int64 + uint32` | `int64` |
| `int64 + uint64` | `int64`, checking the `uint64` |
| `uint16 + uint32` | `uint32` |
| `uint16 + uint64` | `uint64` |
| `uint32 + uint64` | `uint64` |
| `int16 + flt32` | `flt32` |
| `uint16 + flt32` | `flt32` |
| `int32 + flt32` | `flt64` |
| `uint32 + flt32` | `flt64` |
| `int64 + flt32` | `flt64`, checking the integer |
| `uint64 + flt64` | `flt64`, checking the integer |
| `flt32 + flt64` | `flt64` |

The same result type is used for `+`, `-`, `*`, and `/`. `%` uses it only
when both operands are integers. A float operand on `%` is a compile-time
error. Two numeric literals may still narrow to the expected type of the
expression, so `int32 sum = 1 + 2` stays `int32`.

A widening cast that is exact for every value of the source type is a plain
conversion. Any other operand adjustment fails at runtime when the value does
not fit or is not exactly representable:

```text
Runtime numeric error at file:line:col:
value does not fit in 'int64'
```

```text
Runtime numeric error at file:line:col:
value is not exactly representable as 'flt64'
```

## Arithmetic safety

Integer `+`, `-`, `*`, `/`, `%`, `++`, and `--` use checked arithmetic in the
result type. Overflow, including signed minimum divided by `-1`, prints
`graphite: integer overflow` and exits. Division or remainder by zero prints
`graphite: division by zero` and exits. Unsigned unary `-` succeeds only for
zero; any other unsigned value overflows.

`++` and `--` stay limited to integers. Floating-point arithmetic stays
IEEE-754. Float division by zero may produce an infinity or `NaN` and is not
a runtime diagnostic.

## Comparisons

`==`, `!=`, `<`, `<=`, `>`, and `>=` accept every numeric pair and produce
`bln`. `bln` and `str` still compare only with the same type.

- Same-type integers and floats use that type's existing comparison.
- Different integer types compare as exact 128-bit values. A negative signed
  value stays less than every unsigned value. No operand is converted to
  unsigned.
- Different float widths compare as `flt64`. `flt32` widens exactly.
- An integer compared with a float uses the float's exact real value. An
  integer that is not a float value is not equal to that float. `NaN` makes
  ordered comparisons and `==` false, and `!=` true. `-0.0` equals `0.0`.

## Indexing

Every integer width is a valid index. Float indexes are rejected. Signed
indexes keep negative-from-the-end behavior. Unsigned indexes are never
treated as negative. Values are widened exactly to the runtime index type;
they are not truncated. `lengthOf` remains `uint64`, so a signed counter can
be compared with it directly.

A C-style loop may use an assignment as its initializer:

```grf
int i = 0;
uint64 n = lengthOf(arr);
loop (i = 0; i < n; i++) {
    io.println(arr[i]);
}
```
