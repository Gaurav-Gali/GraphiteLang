use crate::codegen::generate;
use crate::lexer::lex;
use crate::manifest::builtin_manifest;
use crate::parser::parse;
use crate::semantic::analyze;

fn analyze_source(source: &str) -> crate::semantic::Analysis {
    let lexed = lex("test.grf", source);
    assert!(lexed.diagnostics.is_empty(), "{:?}", lexed.diagnostics);
    let parsed = parse("test.grf", &lexed.tokens);
    assert!(parsed.is_ok(), "{:?}", parsed.diagnostics);
    analyze(
        "test.grf",
        parsed.program.as_ref().unwrap(),
        &builtin_manifest(),
    )
}

fn messages(source: &str) -> String {
    analyze_source(source)
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn entry_point_diagnostics() {
    assert!(messages("void greet() {\n}\n").contains("missing the required void main()"));
    assert!(messages("void main() {\n}\nvoid main() {\n}\n").contains("duplicate entry point"));
    assert!(messages("int main() {\n    return 0;\n}\n").contains("'main' must return void"));
    assert!(messages("void main(int count) {\n}\n").contains("'main' cannot have parameters"));
}

#[test]
fn duplicate_exact_import_is_rejected() {
    let text = messages("import grf.io;\nimport grf.io;\n\nvoid main() {\n}\n");
    assert!(text.contains("imported more than once"), "{text}");
}

#[test]
fn conflicting_library_name_is_rejected() {
    let text = messages("import grf.io;\nimport other.io;\n\nvoid main() {\n}\n");
    assert!(text.contains("also binds the name 'io'"), "{text}");
}

#[test]
fn unknown_library_and_function() {
    assert!(messages("import grf.unknown;\n\nvoid main() {\n}\n").contains("was not found"));
    assert!(
        messages("import grf.io;\n\nvoid main() {\n    io.unknownFunction(\"Hello\");\n}\n")
            .contains("not exported")
    );
}

#[test]
fn integer_and_float_auto_str_are_accepted() {
    let accepted = analyze_source(
        "import grf.io;\n\nvoid main() {\n    auto str value = 12.433;\n    io.println(value);\n    flt reading = 12.433;\n    io.println(f\"Reading: {reading}\");\n    greet(123);\n}\n\nstr greet(auto str msg) {\n    return f\"Hello {msg}\";\n}\n",
    );
    assert!(
        accepted.diagnostics.is_empty(),
        "{:?}",
        accepted.diagnostics
    );
    let rust = generate(accepted.program.as_ref().unwrap());
    assert!(rust.contains("graphite_core::convert::f64_text"), "{rust}");
    assert_eq!(
        rust.matches("graphite_core::convert::f64_text(").count(),
        2,
        "{rust}"
    );

    let accepted_chr = analyze_source("void main() {\n    auto chr value = true;\n}\n");
    assert!(
        accepted_chr.diagnostics.is_empty(),
        "{:?}",
        accepted_chr.diagnostics
    );
}

#[test]
fn logical_and_bitwise_operators_typecheck() {
    let accepted = analyze_source(
        "\
void main() {
    bln value = true && false || true;
    bln word = false and true or true;
    bln compared = 1 & 2 == 2;
    int bits = 6 & 3 | 1 ^ ~0;
    int shifted = 1 << 2 + 3;
    int n = 6;
    n &= 1;
    n |= 2;
    n ^= 3;
    n <<= 1;
    n >>= 1;
}
",
    );
    assert!(
        accepted.diagnostics.is_empty(),
        "{:?}",
        accepted.diagnostics
    );
    let rust = generate(accepted.program.as_ref().unwrap());
    assert!(rust.contains("if "), "{rust}");
    assert!(
        !rust.contains("let grf_right"),
        "the right operand must stay inside the branch\n{rust}"
    );

    let logical = messages("void main() {\n    bln value = 1 && 2;\n}\n");
    assert!(logical.contains("logical operand"), "{logical}");
    let bitwise = messages("void main() {\n    flt x = 2.0;\n    int value = 1 & x;\n}\n");
    assert!(bitwise.contains("integer operands"), "{bitwise}");
    let tilde = messages("void main() {\n    int value = ~true;\n}\n");
    assert!(tilde.contains("integer operand"), "{tilde}");
}

#[test]
fn ternary_uses_expected_types_and_existing_narrowing() {
    let accepted = analyze_source(
        "\
class Box {
    str name;
    Box(str name) this.name = name;
}

int pick(bln ready) return ready ? 1 : 2;

void main() {
    int n = true ? 1 : 2;
    int? maybe = true ? 1 : null;
    auto int digit = true ? 10.5 : 2;
    bln flag = true && false ? false : true;
    Box? box = null;
    str label = box != null ? box.name : \"\";
    str other = box == null ? \"\" : box.name;
    true ? return;
    loop () {
        true ? break;
    }
}
",
    );
    assert!(
        accepted.diagnostics.is_empty(),
        "{:?}",
        accepted.diagnostics
    );

    let branches = messages("void main() {\n    true ? 1 : 2.5;\n}\n");
    assert!(branches.contains("ternary branches"), "{branches}");
    let ordinary = messages("void main() {\n    flt n = true ? 1 : 2.5;\n}\n");
    assert!(ordinary.contains("type mismatch"), "{ordinary}");
    let condition = messages("void main() {\n    int n = 1 ? 10 : 20;\n}\n");
    assert!(condition.contains("bln"), "{condition}");
    let escaped = messages(
        "\
class Box { str name; Box(str name) this.name = name; }
void main() {
    Box? box = null;
    str label = box != null ? box.name : \"\";
    Box later = box;
}
",
    );
    assert!(escaped.contains("nullable"), "{escaped}");
    let control = messages("void main() {\n    true ? break;\n}\n");
    assert!(control.contains("outside a loop"), "{control}");
}

#[test]
fn sample_program_typechecks() {
    let source = include_str!("../../../../test.grf");
    let lexed = lex("test.grf", source);
    assert!(lexed.diagnostics.is_empty(), "{:?}", lexed.diagnostics);
    let parsed = parse("test.grf", &lexed.tokens);
    assert!(parsed.is_ok(), "{:?}", parsed.diagnostics);
    let analysis = analyze(
        "test.grf",
        parsed.program.as_ref().unwrap(),
        &builtin_manifest(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[test]
fn stage4_decisions() {
    let accepted = analyze_source(
        "\
import grf.io;

int add(int left, int right) {
    return left + right;
}

void main() {
    int32 width = 10;
    int16 depth = 8;
    uint16 seeds = 3;
    flt32 ratio = 1.5;
    int16 lowest = -32768;
    int add = 1;
    io.println(add);
    io.println(add(2, 3));
    const int MAX_RETRIES = 3;
    const auto str label = 42;
    {
        const str APP_NAME = \"Graphite\";
        io.println(APP_NAME);
    }
    flt left = 1.5;
    flt right = 2.5;
    flt sum = left + right;
    flt scaled = left * right - left / right;
    bln ordered = left < right;
    bln same = left == right;
    bln different = left != right;
    io.println(sum);
    io.println(ordered);
    io.println(same);
    io.println(different);
    io.println(scaled);
}
",
    );
    assert!(
        accepted.diagnostics.is_empty(),
        "{:?}",
        accepted.diagnostics
    );
    let rust = generate(accepted.program.as_ref().unwrap());
    assert!(rust.contains("let mut add: i64"), "{rust}");
    assert!(rust.contains("let MAX_RETRIES: i64"), "{rust}");
    assert!(rust.contains("crate::grf_fns::add("), "{rust}");
    assert!(rust.contains("(left + right)"), "{rust}");

    let narrow = messages(
        "\
void main() {
    int32 width = 9999999999;
    int16 depth = 40000;
    uint16 seeds = -1;
    flt32 ratio = 1e100;
    flt32 inexact = 0.1;
    int64 general = width;
}
",
    );
    assert!(narrow.contains("does not fit in 'int32'"), "{narrow}");
    assert!(narrow.contains("does not fit in 'int16'"), "{narrow}");
    assert!(narrow.contains("does not fit in 'uint16'"), "{narrow}");
    assert!(narrow.contains("does not fit in 'flt32'"), "{narrow}");
    assert!(
        narrow.contains("not exactly representable as 'flt32'"),
        "{narrow}"
    );
    assert!(narrow.contains("type mismatch"), "{narrow}");

    let mixed = messages(
        "\
void main() {
    int32 width = 10;
    int64 height = width;
    flt64 wide = 1.5;
    flt32 narrow = wide;
    flt mixed = 1;
    flt32 remainder = 1.5 % 1.5;
}
",
    );
    assert!(mixed.contains("type mismatch"), "{mixed}");
    assert!(mixed.contains("integer operands"), "{mixed}");

    let constants = messages(
        "\
void main() {
    const int limit = 10;
    limit = 20;
    const int size = 5;
    size++;
    loop (const int step = 0; step < 1; step++) {
    }
}
",
    );
    assert!(
        constants.contains("cannot assign to constant"),
        "{constants}"
    );
    assert!(constants.contains("cannot modify constant"), "{constants}");

    let characters = messages("void main() {\n    chr letter = letter;\n}\n");
    assert!(characters.contains("unknown name 'letter'"), "{characters}");

    let calls = messages(
        "\
int add(int left, int right) {
    return left + right;
}

void main() {
    int value = add;
}
",
    );
    assert!(
        calls.contains("expected 'int64', found '(int64, int64) -> int64'"),
        "{calls}"
    );
}

#[test]
fn const_values_can_be_read_copied_and_measured() {
    let accepted = analyze_source(
        "\
void change(int value) {
    value = 4;
}

void main() {
    const int limit = 3;
    const int64 wide = limit;
    const int32 width = 10;
    const int16 depth = 8;
    const uint items = 2;
    const uint64 total = items;
    const uint32 tickets = 4;
    const uint16 seeds = 3;
    const flt reading = 1.5;
    const flt64 same = reading;
    const flt32 ratio = 1.5;
    const bln ready = true;
    const str name = \"Graphite\";
    int copy = limit;
    str text = name;
    change(limit);
    uint64 count = lengthOf(name);
    uint64 literal = lengthOf(\"é\");
    uint64 empty = lengthOf(\"\");
    bln flag = ready;
    flt sum = reading + same;
    int next = wide + copy;
    uint combined = total + items;
    int32 shownWidth = width;
    int16 shownDepth = depth;
    uint32 shownTickets = tickets;
    uint16 shownSeeds = seeds;
    flt32 narrow = ratio;
    text = name;
    flag = ready;
    count = literal + empty;
    sum = reading;
    next = copy;
    combined = items;
    shownWidth = width;
    shownDepth = depth;
    shownTickets = tickets;
    shownSeeds = seeds;
    narrow = ratio;
}
",
    );
    assert!(
        accepted.diagnostics.is_empty(),
        "{:?}",
        accepted.diagnostics
    );
    let rust = generate(accepted.program.as_ref().unwrap());
    assert!(rust.contains("grf_str_length"), "{rust}");
    assert!(rust.contains("let limit: i64"), "{rust}");
}

#[test]
fn const_reassignment_and_length_of_are_rejected() {
    let reassignment = messages(
        "\
void main() {
    const int limit = 3;
    const int64 wide = 1;
    const int32 width = 1;
    const int16 depth = 1;
    const uint items = 1;
    const uint64 total = 1;
    const uint32 tickets = 1;
    const uint16 seeds = 1;
    const flt reading = 1.5;
    const flt64 same = 1.5;
    const flt32 ratio = 1.5;
    const bln ready = true;
    const str name = \"Graphite\";
    limit = 4;
    wide = 4;
    width = 4;
    depth = 4;
    items = 4;
    total = 4;
    tickets = 4;
    seeds = 4;
    reading = 4.0;
    same = 4.0;
    ratio = 4.0;
    ready = false;
    name = \"Other\";
}
",
    );
    for name in [
        "limit", "wide", "width", "depth", "items", "total", "tickets", "seeds", "reading", "same",
        "ratio", "ready", "name",
    ] {
        assert!(
            reassignment.contains(&format!("cannot assign to constant '{name}'")),
            "{reassignment}"
        );
    }

    let lengths = messages(
        "\
void main() {
    int value = 42;
    int64 wide = value;
    int32 width = 1;
    uint items = 1;
    uint64 total = 1;
    flt reading = 1.5;
    flt64 same = reading;
    flt32 ratio = 1.5;
    bln ready = true;
    uint64 a = lengthOf(value);
    uint64 b = lengthOf(wide);
    uint64 c = lengthOf(width);
    uint64 d = lengthOf(items);
    uint64 e = lengthOf(total);
    uint64 f = lengthOf(reading);
    uint64 g = lengthOf(same);
    uint64 h = lengthOf(ratio);
    uint64 i = lengthOf(ready);
    uint64 missing = lengthOf();
    uint64 extra = lengthOf(\"A\", \"B\");
}
",
    );
    for spelling in ["int64", "int32", "uint64", "flt64", "flt32", "bln"] {
        assert!(
            lengths.contains(&format!("'{}' has no defined length", spelling)),
            "{lengths}"
        );
    }
    assert!(lengths.contains("expects 1 argument, found 0"), "{lengths}");
    assert!(lengths.contains("expects 1 argument, found 2"), "{lengths}");

    assert!(
        messages("void main() {\n    int lengthOf = 1;\n}\n").contains("reserved built-in name")
    );
    assert!(
        messages("void hold(int lengthOf) {\n}\n\nvoid main() {\n}\n")
            .contains("reserved built-in name")
    );
    assert!(
        messages("uint64 lengthOf(str value) {\n    return 0;\n}\n\nvoid main() {\n}\n")
            .contains("reserved built-in name")
    );
}

#[test]
fn conversions_share_one_registry() {
    let accepted = analyze_source(
        "\
import grf.io;

void take(auto int value) {
    io.println(value);
}

chr letter(str text) {
    return chr(text);
}

void main() {
    int32 width = 10;
    int16 depth = 8;
    uint16 seeds = 3;
    uint32 tickets = 4;
    auto int parsed = \"123\";
    auto int32 narrow = \"123\";
    auto str label = width;
    auto bln flag = \"true\";
    str text = str(width);
    str floatText = str(flt32(1));
    chr mark = chr(\"A\");
    int code = int(mark);
    uint16 small = uint16(seeds);
    take(\"456\");
    flt reading = 1.5;
    flt64 same = reading;
    flt32 ratio = 1.5;
    int32 still = width;
    io.println(parsed);
    io.println(narrow);
    io.println(label);
    io.println(flag);
    io.println(text);
    io.println(floatText);
    io.println(code);
    io.println(small);
    io.println(letter(\"é\"));
    io.println(depth);
    io.println(tickets);
    io.println(same);
    io.println(ratio);
    io.println(still);
}
",
    );
    assert!(
        accepted.diagnostics.is_empty(),
        "{:?}",
        accepted.diagnostics
    );
    let rust = generate(accepted.program.as_ref().unwrap());
    assert!(
        rust.contains("graphite_core::convert::str_to_signed"),
        "{rust}"
    );
    assert!(
        rust.contains("graphite_core::convert::signed_text"),
        "{rust}"
    );

    let literal = messages(
        "void main() {\n    int32 width = 10;\n    int32 bad = 3000000000;\n    int64 height = width;\n}\n",
    );
    assert!(literal.contains("does not fit in 'int32'"), "{literal}");
    assert!(literal.contains("type mismatch"), "{literal}");
    assert!(!literal.contains("cannot convert"), "{literal}");

    let characters = messages(
        "\
void main() {
    chr mark = chr(\"A\");
    bln flag = bln(mark);
    chr other = chr(true);
}
",
    );
    assert!(characters.is_empty(), "{characters}");

    assert!(messages("void main() {\n    int value = int();\n}\n").contains("expects 1 argument"));
    assert!(
        messages("void main() {\n    int value = void(1);\n}\n")
            .contains("not a conversion target")
    );
    assert!(
        analyze_source("void main() {\n    flt value = flt(\".5\");\n}\n")
            .diagnostics
            .is_empty()
    );
    assert!(
        analyze_source("void main() {\n    auto int32 value = 40000;\n}\n")
            .diagnostics
            .is_empty()
    );
}

#[test]
fn auto_reassignment_uses_the_declared_target() {
    let source = "\
import grf.io;

void take(auto int value) {
    value = \"9\";
    io.println(value);
}

void main() {
    auto int age = \"21\";
    age = \"12\";
    age = 25;
    int next = age + 1;
    auto uint count = \"12\";
    count = \"13\";
    auto flt price = \"12.5\";
    price = \"13.5\";
    auto int64 alias = \"8\";
    alias = \"9\";
    take(\"123\");
    io.println(age);
    io.println(next);
    io.println(count);
    io.println(price);
    io.println(alias);
}
";
    let accepted = analyze_source(source);
    assert!(
        accepted.diagnostics.is_empty(),
        "{:?}",
        accepted.diagnostics
    );
    let rust = generate(accepted.program.as_ref().unwrap());
    assert!(
        rust.matches("graphite_core::convert::str_to_signed")
            .count()
            >= 2,
        "{rust}"
    );
    assert!(rust.contains("let mut age: i64"), "{rust}");

    let ordinary = messages("void main() {\n    int age = 21;\n    age = \"12\";\n}\n");
    assert!(ordinary.contains("type mismatch"), "{ordinary}");
    assert!(!ordinary.contains("cannot convert"), "{ordinary}");

    let unsupported = messages("void main() {\n    auto int age = \"21\";\n    age = [1];\n}\n");
    assert!(
        unsupported.contains("cannot convert 'int64[]' to 'int64'"),
        "{unsupported}"
    );
    let to_chr = messages(
        "\
type Mark { tag: chr }

void main() {
    int[] values = [1];
    chr fromArray = chr(values);
    map<str, int> pairs = {};
    chr fromMap = chr(pairs);
    Mark mark = { tag: chr(\"Z\") };
    chr fromRecord = chr(mark);
    int? number = 6;
    chr fromOptional = chr(number);
    chr digit = chr(7);
}
",
    );
    assert!(to_chr.is_empty(), "{to_chr}");
}

#[test]
fn arrays_keep_static_element_types() {
    let accepted = analyze_source(
        "\
import grf.io;

int[] copy_of(int[] items) {
    return items;
}

void main() {
    int[] values = [10, 20, 30];
    int32[] narrow = [1, 2];
    int[][] matrix = [[1, 2], [3, 4]];
    int[][] empty = [];
    int[][] rows = [[], [1]];
    const int[] fixed = [4, 5];
    int first = fixed[0];
    int[] copied = fixed[:];
    uint64 count = lengthOf(fixed);
    uint64 same = fixed.length();
    int a, const int b = [8, 9];
    loop (int value : values) {
        io.println(value);
    }
    loop (uint64 idx, int value : values.enumerate()) {
        io.println(idx);
        io.println(value);
    }
    int[] returned = copy_of(values);
    int[3] bounded = [];
    bounded.push(1);
    int[3] filled = [1, 2, 3];
    int[2][1] grid = [[4, 5]];
    io.println(first);
    io.println(count);
    io.println(same);
    io.println(a);
    io.println(b);
    io.println(narrow[0]);
    io.println(matrix[1][0]);
    io.println(empty.length());
    io.println(rows[1][0]);
    io.println(copied[0]);
    io.println(returned[0]);
    io.println(bounded.length());
    io.println(filled[2]);
    io.println(grid[0][1]);
}
",
    );
    assert!(
        accepted.diagnostics.is_empty(),
        "{:?}",
        accepted.diagnostics
    );

    let mismatch = messages("void main() {\n    int[] values = [1, \"two\"];\n}\n");
    assert!(mismatch.contains("type mismatch"), "{mismatch}");
    let nested = messages("void main() {\n    int[][] rows = [[1], [\"two\"]];\n}\n");
    assert!(nested.contains("type mismatch"), "{nested}");
    let ordinary = messages("void main() {\n    int value = [1];\n}\n");
    assert!(ordinary.contains("type mismatch"), "{ordinary}");
    assert!(!ordinary.contains("destructur"), "{ordinary}");
    let void_array = messages("void main() {\n    void[] values = [];\n}\n");
    assert!(
        void_array.contains("void is not a valid array element type"),
        "{void_array}"
    );
    let empty = messages(
        "void main() {\n    int[] values = [];\n    int[] missing = values;\n    missing = [];\n}\n",
    );
    assert!(empty.is_empty(), "{empty}");
    let untyped = messages("void main() {\n    io.println([]);\n}\n");
    assert!(
        untyped.contains("needs a known element type")
            || untyped.contains("cannot be interpolated"),
        "{untyped}"
    );
    let equality = messages(
        "void main() {\n    int[] left = [1];\n    int[] right = [1];\n    bln same = left == right;\n}\n",
    );
    assert!(equality.contains("comparable"), "{equality}");
    let interpolated = messages(
        "import grf.io;\n\nvoid main() {\n    int[] values = [1];\n    str text = f\"{values}\";\n    io.println(values);\n    io.println(str(values));\n    io.println(text);\n}\n",
    );
    assert!(interpolated.is_empty(), "{interpolated}");
    let outside = messages(
        "void main() {\n    int[] values = [1];\n    iter<int> it = values.iter();\n    bln more = it.hasNext();\n}\n",
    );
    assert!(outside.is_empty(), "{outside}");
    let index_type = messages(
        "void main() {\n    int[] values = [1];\n    loop (int idx, int value : values.enumerate()) {\n        io.println(value);\n    }\n}\n",
    );
    assert!(index_type.contains("uint64"), "{index_type}");
    let constant = messages(
        "void main() {\n    const int[] values = [1];\n    values[0] = 2;\n    values.push(3);\n}\n",
    );
    assert!(constant.contains("cannot assign to constant"), "{constant}");
    let during = messages(
        "void main() {\n    int[] values = [1, 2];\n    int i = 0;\n    loop (int value : values) {\n        values[i] = value + 1;\n        values.push(0);\n        i++;\n    }\n}\n",
    );
    assert!(during.contains("while it is being iterated"), "{during}");
    let duplicate = messages("void main() {\n    int a, int a = [1, 2];\n}\n");
    assert!(duplicate.contains("duplicate binding"), "{duplicate}");
    let too_long = messages("void main() {\n    int[3] values = [1, 2, 3, 4];\n}\n");
    assert!(too_long.contains("exceeds the maximum of 3"), "{too_long}");
    let nested_limit = messages("void main() {\n    int[2][1] grid = [[1, 2, 3]];\n}\n");
    assert!(
        nested_limit.contains("exceeds the maximum of 2"),
        "{nested_limit}"
    );
    let same_type = messages(
        "\
int[] echo(int[] items) {
    return items;
}

void main() {
    int[] dynamic = [1];
    int[3] bounded = dynamic;
    int[] again = bounded;
    int[5] slot = [];
    slot = echo(slot);
    again = echo(bounded);
}
",
    );
    assert!(same_type.is_empty(), "{same_type}");
    let literal_count = messages("void main() {\n    int a, int b = [1];\n}\n");
    assert!(
        literal_count.contains("expects 2 elements, found 1"),
        "{literal_count}"
    );
    let too_wide = messages("void main() {\n    int32[] values = [4000000000];\n}\n");
    assert!(too_wide.contains("does not fit"), "{too_wide}");
}

#[test]
fn numeric_operators_accept_mixed_families() {
    let accepted = analyze_source(
        "\
import grf.io;

void main() {
    int a = 10;
    uint b = 5;
    flt c = 2.5;
    int32 sum = 1 + 2;
    io.println(a + b);
    io.println(a - b);
    io.println(a * b);
    io.println(a / b);
    io.println(a + c);
    io.println(b * c);
    io.println(c / a);
    io.println(a < b);
    io.println(c > a);
    io.println(c == a);
    int16 i16 = 0;
    int32 i32 = 0;
    int64 i64 = 0;
    uint16 u16 = 0;
    uint32 u32 = 0;
    uint64 u64 = 0;
    int[] arr = [1, 2, 3, 4];
    io.println(arr[i16]);
    io.println(arr[i32]);
    io.println(arr[i64]);
    io.println(arr[u16]);
    io.println(arr[u32]);
    io.println(arr[u64]);
    io.println(arr[-1]);
    loop (uint64 i = 0; i < lengthOf(arr); i++) {
        if (arr[i] % 2 == 0) {
            io.println(arr[i]);
        }
    }
    int i = 0;
    uint64 n = lengthOf(arr);
    loop (i = 0; i < n; i++) {
        io.println(i);
    }
    int16 low = 1;
    int64 high = 2;
    int64 wide = 0;
    wide = low + high;
    io.println(sum);
}
",
    );
    assert!(
        accepted.diagnostics.is_empty(),
        "{:?}",
        accepted.diagnostics
    );

    let destination = messages(
        "\
void main() {
    int a = 1;
    int b = 2;
    flt sum = a + b;
}
",
    );
    assert!(destination.contains("type mismatch"), "{destination}");
    let float_index = messages(
        "\
void main() {
    int[] arr = [1];
    flt index = 0.0;
    int value = arr[index];
}
",
    );
    assert!(
        float_index.contains("an index must be an integer"),
        "{float_index}"
    );
    let remainder =
        messages("void main() {\n    int a = 1;\n    flt b = 2.0;\n    flt c = a % b;\n}\n");
    assert!(remainder.contains("integer operands"), "{remainder}");
}

#[test]
fn codegen_uses_the_manifest_rust_path() {
    let analysis = analyze_source("import grf.io;\n\nvoid main() {\n    io.print(\"Hi\");\n}\n");
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let rust = generate(analysis.program.as_ref().unwrap());
    let manifest = builtin_manifest();
    let path = &manifest.libraries["io"].functions["print"].rust_path;
    assert!(rust.contains(path), "{rust}");
    assert!(!rust.contains("graphite_core::io::println"));
}

#[test]
fn records_keep_nominal_identity() {
    let source = "\
import grf.io;

type Address {
    city: str,
    country: str,
}

type Profile {
    name: str,
    age: int,
    scores: int[],
    address: Address,
}

type Customer {
    name: str,
    age: int,
}

Profile make() {
    return {
        name: \"Ada\",
        age: 36,
        scores: [10, 20],
        address: { city: \"London\", country: \"UK\" },
    };
}

void take(Profile value) {
    io.println(value.name);
}

void main() {
    Profile first = make();
    const Profile fixed = first;
    Profile copy = fixed;
    copy.age = 37;
    copy.scores[0] = 99;
    copy.address.city = \"Paris\";
    Profile[] people = [first];
    people[0].name = \"Grace\";
    uint64 count = lengthOf(fixed);
    uint64 nested = lengthOf(first.address);
    take(first);
    io.println(first.name);
    io.println(first.age);
    io.println(count);
    io.println(nested);
    io.println(copy.age);
    io.println(people[0].name);
}
";
    let accepted = analyze_source(source);
    assert!(
        accepted.diagnostics.is_empty(),
        "{:?}",
        accepted.diagnostics
    );
    let rust = generate(accepted.program.as_ref().unwrap());
    assert!(rust.contains("struct GrfProfile"), "{rust}");
    assert!(rust.contains("struct GrfAddress"), "{rust}");
    assert!(rust.contains("4u64"), "{rust}");
    assert!(!rust.contains("derive(PartialEq)"), "{rust}");

    let mismatch = messages(
        "\
type User { name: str, age: int, }
type Customer { name: str, age: int, }
void main() {
    User user = { name: \"Ada\", age: 36 };
    Customer customer = user;
}
",
    );
    assert!(mismatch.contains("type mismatch"), "{mismatch}");
    assert!(mismatch.contains("User"), "{mismatch}");
    assert!(mismatch.contains("Customer"), "{mismatch}");

    let converted = messages(
        "\
type User { name: str, age: int, }
type Customer { name: str, age: int, }
void main() {
    Customer customer = { name: \"Ada\", age: 36 };
    auto User user = customer;
}
",
    );
    assert!(
        converted.contains("cannot convert 'Customer' to 'User'"),
        "{converted}"
    );

    let call = messages(
        "\
type User { name: str, age: int, }
void main() {
    User user = User({ name: \"Ada\", age: 36 });
}
",
    );
    assert!(call.contains("unknown function 'User'"), "{call}");

    let fields = messages(
        "\
type User { name: str, age: int, }
void main() {
    User missing = { name: \"Ada\" };
    User unknown = { name: \"Ada\", age: 36, active: true };
    User duplicate = { name: \"Ada\", name: \"Grace\", age: 36 };
    User wrong = { name: 123, age: \"21\" };
}
",
    );
    assert!(fields.contains("missing field 'age'"), "{fields}");
    assert!(fields.contains("unknown field 'active'"), "{fields}");
    assert!(fields.contains("duplicate field 'name'"), "{fields}");
    assert!(fields.contains("type mismatch"), "{fields}");

    let declarations = messages(
        "\
type User { name: str, age: int, }
type User { name: str, }
type Empty { }
type Node { next: Missing, }
void main() {
}
",
    );
    assert!(
        declarations.contains("already declared at line"),
        "{declarations}"
    );
    assert!(
        declarations.contains("at least one field"),
        "{declarations}"
    );
    assert!(
        declarations.contains("unknown type 'Missing'"),
        "{declarations}"
    );

    let void_field = messages("type User {\n    value: void,\n}\n\nvoid main() {\n}\n");
    assert!(
        void_field.contains("void is not a valid field type"),
        "{void_field}"
    );

    let direct = messages("type Node {\n    next: Node,\n}\n\nvoid main() {\n}\n");
    assert!(
        direct.contains("record type cycle: Node -> Node"),
        "{direct}"
    );
    let indirect =
        messages("type A {\n    child: B,\n}\n\ntype B {\n    parent: A,\n}\n\nvoid main() {\n}\n");
    assert!(indirect.contains("record type cycle:"), "{indirect}");
    let through_array = messages(
        "type A {\n    items: B[],\n}\n\ntype B {\n    parent: A,\n}\n\nvoid main() {\n}\n",
    );
    assert!(
        through_array.contains("record type cycle:"),
        "{through_array}"
    );

    let writes = messages(
        "\
type User { name: str, age: int, scores: int[], }
User make() {
    return { name: \"Ada\", age: 36, scores: [1] };
}
void main() {
    const User fixed = make();
    fixed.age = 37;
    fixed.scores[0] = 2;
    make().age = 37;
}
",
    );
    assert!(
        writes.contains("cannot assign to constant 'fixed'"),
        "{writes}"
    );
    assert!(
        writes.contains("a field assignment requires a mutable variable"),
        "{writes}"
    );

    let equality = messages(
        "\
type User { name: str, age: int, }
void main() {
    User left = { name: \"Ada\", age: 36 };
    User right = left;
    bln same = left == right;
    bln different = left != right;
}
",
    );
    assert!(
        equality.contains("cannot compare record values"),
        "{equality}"
    );

    let printing = analyze_source(
        "\
import grf.io;
type User { name: str, age: int, }
void main() {
    User user = { name: \"Ada\", age: 36 };
    io.println(user);
    io.println(f\"{user}\");
    str text = str(user);
    io.println(text);
}
",
    );
    assert!(
        printing.diagnostics.is_empty(),
        "{:?}",
        printing.diagnostics
    );
    let rust = generate(printing.program.as_ref().unwrap());
    assert!(rust.contains("name: "), "{rust}");
    assert!(rust.contains("age: "), "{rust}");

    let not_numeric = messages(
        "\
type User { name: str, age: int, }
void main() {
    User user = { name: \"Ada\", age: 36 };
    auto int value = user;
}
",
    );
    assert!(
        not_numeric.contains("cannot convert 'User' to 'int64'"),
        "{not_numeric}"
    );
}

#[test]
fn maps_keep_static_key_and_value_types() {
    let program = analyze_source(
        "\
import grf.io;
type User { name: str, age: int, }
map<str, int> supply() { return {}; }
void main() {
    map<str, int> scores = { \"Ada\": 10, \"Linus\": 20, };
    map<int64, str> same = {};
    map<int, str> aliased = same;
    map<str, map<str, int>> nested = { \"group\": { \"count\": 1 } };
    map<str, int[3]> histories = { \"Ada\": [1, 2, 3] };
    map<str, User> users = { \"Ada\": { name: \"Ada\", age: 20 } };
    scores[\"Ada\"] = 11;
    users[\"Ada\"].age = 22;
    histories[\"Ada\"][0] = 9;
    nested[\"group\"][\"count\"] = 3;
    int kept = scores.getOrDefault(\"Ada\", 0);
    str[] names = scores.keys();
    int total = 0;
    loop (str key, int value : scores.entries()) {
        total = total + value;
    }
    io.println(scores);
    io.println(str(scores));
    io.println(f\"{scores}\");
    io.println(lengthOf(scores));
    io.println(kept);
    io.println(lengthOf(names));
    io.println(total);
    io.println(aliased);
}
",
    );
    assert!(program.diagnostics.is_empty(), "{:?}", program.diagnostics);
    let rust = generate(program.program.as_ref().unwrap());
    assert!(rust.contains("std::collections::HashMap"), "{rust}");
    assert!(!rust.contains("BuildHasher"), "{rust}");

    let rejected = [
        (
            "void main() { map<void, int> bad = {}; }\n",
            "'void' cannot be a map key",
        ),
        (
            "void main() { map<str, void> bad = {}; }\n",
            "void is not a valid map value type",
        ),
        (
            "void main() { map<flt, int> bad = {}; }\n",
            "'flt64' cannot be a map key",
        ),
        (
            "void main() { map<int[], str> bad = {}; }\n",
            "'int64[]' cannot be a map key",
        ),
        (
            "void main() { map<map<str, int>, str> bad = {}; }\n",
            "'map<str, int64>' cannot be a map key",
        ),
        (
            "type User { name: str, } void main() { map<User, int> bad = {}; }\n",
            "'User' cannot be a map key",
        ),
        (
            "void main() { map<str, int> scores = { \"Ada\": 1, \"Bob\": true }; }\n",
            "type mismatch: expected 'int64', found 'bln'",
        ),
        (
            "void main() { map<str, int> scores = {}; int value = scores[1]; }\n",
            "type mismatch: expected 'str', found 'int64'",
        ),
        (
            "void main() { map<str, int> scores = {}; scores[\"Ada\"] = true; }\n",
            "type mismatch: expected 'int64', found 'bln'",
        ),
        (
            "void main() { map<str, int> scores = {}; int value = scores.getOrDefault(\"Ada\", \"no\"); }\n",
            "type mismatch: expected 'int64', found 'str'",
        ),
        (
            "void main() { map<int, int> source = {}; map<str, int> scores = { ...source }; }\n",
            "type mismatch: expected 'map<str, int64>', found 'map<int64, int64>'",
        ),
        (
            "void main() { const map<str, int> scores = {}; scores[\"Ada\"] = 1; }\n",
            "cannot assign to constant 'scores'",
        ),
        (
            "void main() { const map<str, int> scores = {}; scores.set(\"Ada\", 1); }\n",
            "cannot assign to constant 'scores'",
        ),
        (
            "void main() { const map<str, int> scores = {}; bln removed = scores.remove(\"Ada\"); }\n",
            "cannot assign to constant 'scores'",
        ),
        (
            "type User { age: int, } void main() { const map<str, User> users = { \"Ada\": { age: 1 } }; users[\"Ada\"].age = 2; }\n",
            "cannot assign to constant 'users'",
        ),
        (
            "void main() { map<str, int> scores = {}; map<str, int> slice = scores[0:1]; }\n",
            "maps do not support slicing",
        ),
        (
            "void main() { map<str, int> scores = {}; loop (str key : scores.entries()) {} }\n",
            "this loop expects 2 bindings, found 1",
        ),
        (
            "void main() { map<str, int> scores = {}; loop (int key, str value : scores.entries()) {} }\n",
            "type mismatch: expected 'str', found 'int64'",
        ),
        (
            "map<str, int> make() { return {}; } void main() { make()[\"Ada\"] = 1; }\n",
            "a map assignment requires a mutable variable",
        ),
        (
            "map<str, int> make() { return {}; } void main() { make().set(\"Ada\", 1); }\n",
            "map method 'set' requires a mutable variable",
        ),
        (
            "void main() { map<str, int> scores = {}; scores.foo(); }\n",
            "map has no method 'foo'",
        ),
        (
            "void main() { map<str, int> scores = {}; int value = scores.get(); }\n",
            "method 'get' expects 1 argument, found 0",
        ),
        (
            "void main() { map<str, int> scores = {}; bln same = scores == scores; }\n",
            "operator '==' cannot compare map values",
        ),
        (
            "void main() { map<str, int> scores = {}; bln different = scores != scores; }\n",
            "operator '!=' cannot compare map values",
        ),
        (
            "void main() { map<str, int> scores = {}; scores.entries(); }\n",
            "entries() can only be used as a loop source",
        ),
        (
            "type User { name: str, } void main() { User user = { ...user }; }\n",
            "spread is only allowed in a map literal",
        ),
        (
            "void main() { map<str, int> scores = { \"Ada\": 1 }; }\n",
            "",
        ),
        (
            "void main() { int value = { \"Ada\": 1 }; }\n",
            "a brace literal needs an expected record or map type",
        ),
    ];
    for (source, snippet) in rejected {
        if snippet.is_empty() {
            let analysis = analyze_source(source);
            assert!(
                analysis.diagnostics.is_empty(),
                "{source}\n{:?}",
                analysis.diagnostics
            );
            continue;
        }
        let text = messages(source);
        assert!(
            text.contains(snippet),
            "{source}\nexpected {snippet}\n{text}"
        );
    }
}

#[test]
fn type_descriptors_use_canonical_identity() {
    let program = analyze_source(
        "\
import grf.io;

type User { name: str, age: int, }
type Account { name: str, age: int, }
type Metadata { descriptor: Type, }

int Type() {
    return 7;
}

void take(auto str value) {
    io.println(value);
}

void main() {
    int value = Type();
    Type number = typeOf(value);
    Type text = typeOf(\"hello\");
    int[] ages = [value];
    int[3] fixed = [1, 2, 3];
    map<str, int> scores = { \"Ada\": 10 };
    map<str, int64> same = { \"Ada\": 10 };
    User user = { name: \"Ada\", age: 36 };
    Account account = { name: \"Ada\", age: 36 };
    User[] users = [user];
    map<str, User> byName = { \"Ada\": user };
    Type nested = typeOf(typeOf(1));
    Type agesType = typeOf(ages);
    Type fixedType = typeOf(fixed);
    Type scoresType = typeOf(scores);
    Type usersType = typeOf(users);
    Type byNameType = typeOf(byName);
    bln aliases = typeOf(scores) == typeOf(same);
    bln different = typeOf(user) != typeOf(account);
    bln compound = typeOf(user) != typeOf(users);
    io.println(number.name());
    io.println(number);
    io.println(str(number));
    io.println(f\"{number}\");
    auto str shown = number;
    shown = text;
    take(typeOf(user));
    Type[] descriptors = [number, text];
    map<str, Type> descriptorByName = { \"age\": number, \"name\": text };
    Metadata metadata = { descriptor: number };
    Type copied = metadata.descriptor;
    const Type frozen = number;
    io.println(aliases);
    io.println(different);
    io.println(compound);
    io.println(nested.name());
    io.println(descriptors[0].name());
    io.println(descriptorByName.get(\"age\").name());
    io.println(copied == frozen);
}
",
    );
    assert!(program.diagnostics.is_empty(), "{:?}", program.diagnostics);
    let rust = generate(program.program.as_ref().unwrap());
    for name in ["int64[]", "map<str, int64>", "User[]", "map<str, User>"] {
        assert!(rust.contains(name), "{name} missing from {rust}");
    }

    let rejected = [
        (
            "type Type { value: int, }\nvoid main() {\n}\n",
            "'Type' is a built-in type",
        ),
        (
            "void main() {\n    int typeOf = 1;\n}\n",
            "'typeOf' is a reserved built-in name",
        ),
        (
            "void hold(int typeOf) {\n}\n\nvoid main() {\n}\n",
            "'typeOf' is a reserved built-in name",
        ),
        (
            "uint64 typeOf(str value) {\n    return 0;\n}\n\nvoid main() {\n}\n",
            "'typeOf' is a reserved built-in name",
        ),
        (
            "type Note { typeOf: int, }\nvoid main() {\n}\n",
            "'typeOf' is a reserved built-in name",
        ),
        (
            "type typeOf { value: int, }\nvoid main() {\n}\n",
            "'typeOf' is a reserved built-in name",
        ),
        (
            "void main() {\n    int[] items = [1, 2];\n    int typeOf, int other = items;\n}\n",
            "'typeOf' is a reserved built-in name",
        ),
        (
            "void main() {\n    int[] items = [1];\n    loop (int typeOf : items) {\n    }\n}\n",
            "'typeOf' is a reserved built-in name",
        ),
        (
            "void main() {\n    Type t = typeOf();\n}\n",
            "typeOf expects 1 argument, found 0",
        ),
        (
            "void main() {\n    Type t = typeOf(1, 2);\n}\n",
            "typeOf expects 1 argument, found 2",
        ),
        (
            "void noop() {\n}\n\nvoid main() {\n    Type t = typeOf(noop());\n}\n",
            "typeOf requires a value",
        ),
        (
            "void main() {\n    Type t = typeOf(missing);\n}\n",
            "unknown name 'missing'",
        ),
        (
            "void main() {\n    Type t = typeOf(1);\n    str name = t.other();\n}\n",
            "Type has no method 'other'",
        ),
        (
            "void main() {\n    Type t = typeOf(1);\n    str name = t.name(1);\n}\n",
            "name expects 0 arguments, found 1",
        ),
        (
            "void main() {\n    Type t = typeOf(1);\n    str name = t.name;\n}\n",
            "'Type' has no field 'name'",
        ),
        (
            "void main() {\n    str text = typeOf(1);\n}\n",
            "type mismatch: expected 'str', found 'Type'",
        ),
        (
            "void exact(str value) {\n}\n\nvoid main() {\n    exact(typeOf(1));\n}\n",
            "type mismatch: expected 'str', found 'Type'",
        ),
        (
            "void main() {\n    Type a = typeOf(1);\n    Type b = typeOf(\"hello\");\n    bln invalid = a < b;\n}\n",
            "operator '<' cannot compare type descriptors",
        ),
        (
            "void main() {\n    Type a = typeOf(1);\n    bln invalid = a <= a;\n}\n",
            "operator '<=' cannot compare type descriptors",
        ),
        (
            "void main() {\n    Type a = typeOf(1);\n    bln invalid = a > a;\n}\n",
            "operator '>' cannot compare type descriptors",
        ),
        (
            "void main() {\n    Type a = typeOf(1);\n    bln invalid = a >= a;\n}\n",
            "operator '>=' cannot compare type descriptors",
        ),
        (
            "void main() {\n    Type a = typeOf(1);\n    bln invalid = a == \"int64\";\n}\n",
            "operator '==' cannot compare 'Type' with 'str'",
        ),
        (
            "void main() {\n    Type a = typeOf(1);\n    bln invalid = a != 1;\n}\n",
            "operator '!=' cannot compare 'Type' with 'int64'",
        ),
        (
            "void main() {\n    map<Type, str> names = {};\n}\n",
            "'Type' cannot be a map key",
        ),
        (
            "void main() {\n    uint64 count = lengthOf(typeOf(1));\n}\n",
            "'Type' has no defined length",
        ),
    ];
    for (source, snippet) in rejected {
        let text = messages(source);
        assert!(
            text.contains(snippet),
            "{source}\nexpected {snippet}\n{text}"
        );
    }
}

#[test]
fn reserved_identifiers_cannot_be_declaration_names() {
    for name in ["self", "crate", "Self"] {
        let message = format!("'{name}' is a reserved name");
        let rejected = [
            format!("void {name}() {{\n}}\n\nvoid main() {{\n}}\n"),
            format!("void main() {{\n    int {name} = 1;\n}}\n"),
            format!("void hold(int {name}) {{\n}}\n\nvoid main() {{\n}}\n"),
            format!("type Note {{\n    {name}: int,\n}}\n\nvoid main() {{\n}}\n"),
            format!(
                "void main() {{\n    int[] items = [1, 2];\n    int {name}, int other = items;\n}}\n"
            ),
            format!(
                "void main() {{\n    int[] items = [1];\n    loop (int {name} : items) {{\n    }}\n}}\n"
            ),
        ];
        for source in rejected {
            let text = messages(&source);
            assert!(
                text.contains(&message),
                "{source}\nexpected {message}\n{text}"
            );
        }
    }
}

#[test]
fn auto_fields_convert_through_the_record_slot() {
    let program = analyze_source(
        "\
import grf.io;

void main() {
    rectangle r1 = {
        height: 10,
        width: \"200\",
    };
    r1.width = \"3\";
    io.println(area(r1));
    io.println(area({ height: \"4\", width: 5 }));
}

type rectangle {
    height: auto int,
    width: auto int,
}

flt area(auto rectangle rect) {
    return flt(rect.height * rect.width);
}
",
    );
    assert!(program.diagnostics.is_empty(), "{:?}", program.diagnostics);

    let exact = messages(
        "\
type rectangle { height: int, width: int, }
void main() {
    rectangle r1 = { height: 10, width: \"200\" };
}
",
    );
    assert!(
        exact.contains("type mismatch: expected 'int64', found 'str'"),
        "{exact}"
    );

    let other = messages(
        "\
type rectangle { height: auto int, width: auto int, }
type square { height: auto int, width: auto int, }
flt area(auto rectangle rect) { return flt(rect.height * rect.width); }
void main() {
    square box = { height: 1, width: 2 };
    flt value = area(box);
}
",
    );
    assert!(
        other.contains("cannot convert 'square' to 'rectangle'"),
        "{other}"
    );
}

#[test]
fn try_requires_a_handler_and_known_catch_types() {
    let bare = analyze_source("void main() {\n    try {\n    }\n}\n");
    assert!(
        bare.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "GRF-EXC-001"),
        "{:?}",
        bare.diagnostics
    );
    assert!(
        messages("void main() {\n    try {\n    }\n}\n")
            .contains("a try statement needs a catch or a finally")
    );
    assert!(
        messages("void main() {\n    try {\n    } catch (Nope error) {\n    }\n}\n")
            .contains("unknown catch type 'Nope'")
    );
    assert!(messages(
        "void main() {\n    try {\n    } catch (RuntimeError error) {\n    } catch (IoError other) {\n    }\n}\n"
    )
    .contains("unreachable catch 'IoError'"));
    assert!(messages(
        "void main() {\n    try {\n    } catch (Error error) {\n    } catch (DivisionByZeroError other) {\n    }\n}\n"
    )
    .contains("unreachable catch 'DivisionByZeroError'"));
    assert!(messages(
        "void main() {\n    try {\n    } catch (Error error) {\n    } catch (RuntimeError other) {\n    }\n}\n"
    )
    .contains("unreachable catch 'RuntimeError'"));
    let broad = messages(
        "void main() {\n    try {\n    } catch (DivisionByZeroError error) {\n    } catch (Error error) {\n    }\n}\n",
    );
    assert!(broad.is_empty(), "{broad}");
    assert!(messages(
        "void main() {\n    try {\n    } catch (IoError error) {\n    } catch (IoError other) {\n    }\n}\n"
    )
    .contains("unreachable catch 'IoError'"));
    let returned = messages(
        "int value() {\n    try {\n        return 1;\n    } finally {\n        return 2;\n    }\n}\n\nvoid main() {\n}\n",
    );
    assert!(
        returned.contains("return is not allowed inside finally"),
        "{returned}"
    );
    let broken = messages(
        "void main() {\n    try {\n    } finally {\n        loop () {\n            break;\n        }\n    }\n}\n",
    );
    assert!(
        broken.contains("break is not allowed inside finally"),
        "{broken}"
    );
    let continued = messages(
        "void main() {\n    try {\n    } finally {\n        loop () {\n            continue;\n        }\n    }\n}\n",
    );
    assert!(
        continued.contains("continue is not allowed inside finally"),
        "{continued}"
    );
    let hidden = messages(
        "import grf.io;\n\nvoid main() {\n    try {\n    } catch (RuntimeError error) {\n    } finally {\n        io.println(error.message());\n    }\n}\n",
    );
    assert!(hidden.contains("unknown name 'error'"), "{hidden}");
    let shared = messages(
        "void main() {\n    try {\n    } catch (DivisionByZeroError error) {\n    } catch (RuntimeError error) {\n    }\n}\n",
    );
    assert!(shared.is_empty(), "{shared}");
    let record = messages(
        "type DivisionByZeroError {\n    value: int,\n}\n\nvoid main() {\n    DivisionByZeroError item = { value: 1 };\n    try {\n    } catch (DivisionByZeroError error) {\n    }\n}\n",
    );
    assert!(record.is_empty(), "{record}");
}

fn diagnostic_text(source: &str) -> String {
    analyze_source(source)
        .diagnostics
        .iter()
        .map(|diagnostic| format!("{} {}", diagnostic.code, diagnostic.message))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn classes_are_reference_types_with_checked_construction() {
    let program = "\
class Counter {
    public int value;
    Counter(int start) {
        this.value = start;
    }
    public int getValue() {
        return this.value;
    }
    public void setValue(int next) {
        this.value = next;
    }
}

int Counter(int start) {
    return start;
}

void main() {
    Counter first = new Counter(1);
    Counter alias = first;
    io.println(first == alias);
    io.println(first != new Counter(2));
}
";
    let missing_import = diagnostic_text(program);
    assert!(
        missing_import.contains("unknown name 'io'"),
        "{missing_import}"
    );

    let source = "\
import grf.io;

class Counter {
    public int value;
    Counter(int start) {
        this.value = start;
    }
    public int getValue() {
        return this.value;
    }
    public void setValue(int next) {
        this.value = next;
    }
}

int Counter(int start) {
    return start;
}

class Early {
    int value;
    Early(bln flag) {
        if (flag) {
            this.value = 1;
        } else {
            this.value = 2;
        }
    }
}

class Node {
    public Node next;
    Node() {
        Node local = this;
        this.next = local;
    }
}

class Open {
    str name = \"Ada\";
}

void main() {
    Counter first = new Counter(1);
    Counter alias = first;
    io.println(first == alias);
    io.println(first.getValue());
    alias.setValue(4);
    Open person = new Open();
    io.println(person.name);
}
";
    let accepted = analyze_source(source);
    assert!(
        accepted.diagnostics.is_empty(),
        "{:?}",
        accepted.diagnostics
    );
    let rust = generate(accepted.program.as_ref().unwrap());
    assert!(rust.contains("graphite_core::heap::alloc"), "{rust}");
    assert!(rust.contains("graphite_core::heap::Rooted"), "{rust}");
    assert!(rust.contains("grf_new_Counter"), "{rust}");
    assert!(!rust.contains("Rc<"), "{rust}");
    assert!(!rust.contains("grf_fns::Counter("), "{rust}");

    let hidden = diagnostic_text(
        "\
class Secret {
    private int value = 1;
    private int read() {
        return this.value;
    }
    private Secret() {
    }
    public Secret make() {
        return new Secret();
    }
}

void main() {
    Secret item = new Secret();
    int leaked = item.value;
    int called = item.read();
}
",
    );
    assert!(
        hidden.contains("GRF-SEM-023 field 'value' is not accessible"),
        "{hidden}"
    );
    assert!(
        hidden.contains("GRF-SEM-023 method 'read' is not accessible"),
        "{hidden}"
    );
    assert!(
        hidden.contains("GRF-SEM-023 constructor of 'Secret' is not accessible"),
        "{hidden}"
    );
    assert_eq!(hidden.matches("not accessible").count(), 3, "{hidden}");

    let unready = diagnostic_text(
        "\
class Slot {
    int value;
}

class Gap {
    Slot item;
    Gap() {
        Slot early = this.item;
        this.touch();
        this.item = new Slot();
    }
    void touch() {
    }
}

void main() {
}
",
    );
    assert!(
        unready.contains("GRF-SEM-024 field 'item' is read before it is initialized"),
        "{unready}"
    );
    assert!(
        unready
            .contains("GRF-SEM-024 cannot call method 'touch' before every field is initialized"),
        "{unready}"
    );

    let partial = diagnostic_text(
        "\
class Slot {
    int value;
}

class Gap {
    Slot item;
    Gap(bln flag) {
        if (flag) {
            this.item = new Slot();
        }
    }
}

void main() {
}
",
    );
    assert!(
        partial.contains("GRF-SEM-024 field 'item' is not initialized"),
        "{partial}"
    );

    let looping = diagnostic_text(
        "\
class Slot {
    int value;
}

class Gap {
    Slot item;
    Gap() {
        loop () {
            this.item = new Slot();
            break;
        }
    }
}

void main() {
}
",
    );
    assert!(
        looping.contains("GRF-SEM-024 field 'item' is not initialized"),
        "{looping}"
    );

    let escaped = diagnostic_text(
        "\
class Node {
    Node next;
    Node() {
        Node[] items = [this];
        this.next = this;
    }
}

void take(Node node) {
}

class Leaky {
    Node next;
    Leaky() {
        take(this);
        this.next = new Node();
    }
}

void main() {
}
",
    );
    assert!(
        escaped.contains("GRF-SEM-025 this cannot escape before construction finishes"),
        "{escaped}"
    );

    let constant = diagnostic_text(
        "\
class Box {
    const int value = 1;
    Box() {
        this.value = 2;
    }
}

void main() {
    const Box item = new Box();
    item.value = 3;
}
",
    );
    assert!(
        constant.contains("GRF-SEM-001 cannot assign to constant field 'value'"),
        "{constant}"
    );
    assert!(
        constant.contains("GRF-SEM-001 cannot assign to constant 'item'"),
        "{constant}"
    );

    let frozen_method = diagnostic_text(
        "\
class Counter {
    int value = 0;
    Counter() {
    }
    public void setValue(int next) {
        this.value = next;
    }
}

void main() {
    const Counter item = new Counter();
    item.setValue(1);
}
",
    );
    assert!(
        frozen_method
            .contains("GRF-SEM-001 cannot call mutating method 'setValue' on constant 'item'"),
        "{frozen_method}"
    );

    let compared = diagnostic_text(
        "\
class Counter {
    int value = 0;
}

class Other {
    int value = 0;
}

void main() {
    Counter left = new Counter();
    Other right = new Other();
    bln same = left == right;
    bln ordered = left < left;
}
",
    );
    assert!(
        compared.contains("GRF-TYP-002 operator '==' cannot compare 'Counter' with 'Other'"),
        "{compared}"
    );
    assert!(
        compared.contains("GRF-TYP-002 operator '<' cannot compare class values"),
        "{compared}"
    );

    let keyed = diagnostic_text(
        "\
class Counter {
    int value = 0;
}

void main() {
    map<Counter, int> scores = {};
}
",
    );
    assert!(
        keyed.contains("GRF-TYP-006 'Counter' cannot be a map key"),
        "{keyed}"
    );

    let printed = diagnostic_text(
        "\
import grf.io;

class Counter {
    int value = 0;
}

void main() {
    Counter item = new Counter();
    io.println(item);
    io.println(str(item));
    io.println(f\"{item}\");
    uint64 size = lengthOf(item);
}
",
    );
    assert!(
        printed.contains("cannot convert 'Counter' to 'str'"),
        "{printed}"
    );
    assert!(
        printed.contains(
            "GRF-TYP-008 lengthOf requires a str, array, record, or map value; 'Counter' has no defined length"
        ),
        "{printed}"
    );

    let arity = diagnostic_text(
        "\
class Counter {
    int value = 0;
}

void main() {
    Counter item = new Counter(1);
}
",
    );
    assert!(
        arity.contains("GRF-SEM-016 expected 0 arguments, found 1"),
        "{arity}"
    );

    let collision = diagnostic_text(
        "\
class Counter {
    int value = 0;
}

type Counter {
    value: int,
}

void main() {
}
",
    );
    assert!(
        collision.contains("GRF-NAM-002 record type 'Counter' is already declared"),
        "{collision}"
    );

    let defaults = diagnostic_text(
        "\
import grf.io;

type Point {
    x: flt,
    y: flt,
}

class Origin {
    Point point;
    int count;
    Origin() {
    }
    flt x() {
        return this.point.x;
    }
}

void main() {
    int count;
    count = 10;
    str name;
    Origin here = new Origin();
    io.println(count);
    io.println(name);
    io.println(here.x());
    io.println(here.count);
}
",
    );
    assert!(defaults.is_empty(), "{defaults}");

    let class_value = diagnostic_text(
        "\
class Counter {
    int value;
}

void main() {
    Counter item;
    item = item;
}
",
    );
    assert!(
        class_value.contains("GRF-SEM-024 variable 'item' is used before it is assigned"),
        "{class_value}"
    );
}

#[test]
fn class_record_fields_can_be_assigned_directly() {
    let nested = diagnostic_text(
        "\
import grf.io;

type rect {
    height: auto flt,
    width: auto flt,
    area: auto flt,
}

class Calc {
    public rect r;
    Calc(auto flt h, auto flt w) {
        this.r.height = h;
        this.r.width = w;
    }
    Calc area() {
        this.r.area = this.r.height * this.r.width;
        return this;
    }
    rect get() {
        return this.r;
    }
}

void main() {
    Calc c = new Calc(2.0, 5.0);
    c.r.height = 4.0;
    io.println(c.area().get());
}
",
    );
    assert!(nested.is_empty(), "{nested}");

    let constant = diagnostic_text(
        "\
type rect {
    height: auto flt,
}

class Locked {
    const rect r;
    Locked() {
        this.r.height = 1.0;
    }
}

void main() {
    const Locked item = new Locked();
    item.r.height = 2.0;
}
",
    );
    assert!(
        constant.contains("GRF-SEM-001 cannot assign to constant field 'r'"),
        "{constant}"
    );
    assert!(
        constant.contains("GRF-SEM-001 cannot assign to constant 'item'"),
        "{constant}"
    );

    let hidden = diagnostic_text(
        "\
type rect {
    height: auto flt,
}

class Hidden {
    private rect r;
}

void main() {
    Hidden item = new Hidden();
    item.r.height = 1.0;
}
",
    );
    assert!(
        hidden.contains("GRF-SEM-023 field 'r' is not accessible"),
        "{hidden}"
    );
}

#[test]
fn nullable_values_default_to_null_and_classes_can_be_assigned_later() {
    let program = diagnostic_text(
        "\
import grf.io;

class node {
    int value;
    node? next;
    node(int value) {
        this.value = value;
        this.next = null;
    }
    int read() {
        return this.value;
    }
}

void main() {
    int? number;
    number = null;
    number = 4;
    node item;
    item = new node(1);
    io.println(item.read());
    node? link = null;
    link = new node(2);
    if (link != null) {
        io.println(link.read());
        link.next = null;
    }
    node head = new node(1);
    head.next = new node(2);
    head.next.next = new node(3);
    io.println(head.next.value);
}
",
    );
    assert!(program.is_empty(), "{program}");

    let bare = diagnostic_text(
        "\
void main() {
    int? number = null;
    int plain = null;
}
",
    );
    assert!(
        bare.contains("GRF-SEM-026 null requires a nullable type"),
        "{bare}"
    );

    let unchecked = diagnostic_text(
        "\
class node {
    int value;
    node(int value) {
        this.value = value;
    }
}

void main() {
    node? link = null;
    node item = link;
}
",
    );
    assert!(
        unchecked.contains("GRF-SEM-026 a nullable value must be checked before it is used"),
        "{unchecked}"
    );
}

#[test]
fn lambdas_store_callables_and_reject_captures() {
    let valid = analyze_source(
        "\
import grf.io;

int apply(int value, int operation(auto int x)) return operation(value);
int negate(int x) return 0 - x;
int add(int a, int b) return a + b;

void main() {
    auto int square = int (int x) return x ** 2;
    int result = square(5);
    int converted = apply(9, int (int x) return x * x);
    int negative = apply(12, negate);
    square = int (int x) return x + 1;
    auto int add_ten = int (int x) return add(x, 10);
    io.println(add_ten(result));
    io.println(converted);
    io.println(negative);
}
",
    );
    assert!(
        valid.diagnostics.is_empty(),
        "{:?}",
        valid
            .diagnostics
            .iter()
            .map(|diagnostic| &diagnostic.message)
            .collect::<Vec<_>>()
    );

    let rejected = messages(
        "\
int apply(int value, int operation(auto int x)) {
    return operation(value);
}

void main() {
    int factor = 10;
    auto int multiply = int (int x) return x * factor;
    int plain = int (int x) return x ** 2;
    int bad = apply(5, int (flt x) return x ** 2);
    auto int fact = int (int n) {
        if (n <= 1) return 1;
        return n * fact(n - 1);
    };
    auto int square = int (int x) return x ** 2;
    int n = square;
}
",
    );
    assert!(rejected.contains("cannot capture 'factor'"), "{rejected}");
    assert!(rejected.contains("cannot capture 'fact'"), "{rejected}");
    assert!(
        rejected.contains("cannot convert a lambda to 'int64'"),
        "{rejected}"
    );
    assert!(rejected.contains("flt64"), "{rejected}");
    assert!(
        rejected.contains("expected 'int64', found '(flt64) -> int64'")
            || rejected.contains("expected '(int64) -> int64'"),
        "{rejected}"
    );
    assert!(
        rejected.contains("expected 'int64', found '(int64) -> int64'"),
        "{rejected}"
    );
}

#[test]
fn inheritance_requires_matching_overrides() {
    let accepted = messages(
        "\
abstract class Animal {
    str name;
    Animal(str name) { this.name = name; }
    @abstract void speak();
}
class Dog extends Animal {
    Dog(str name) { super(name); }
    @override void speak() {}
}
interface Printable { void print(); }
class Report implements Printable {
    @override void print() {}
}
class Formatter {
    str format(int value) { return \"int\"; }
    str format(str value) { return value; }
}
void main() {
    Dog dog = Dog(\"Buddy\");
    Animal animal = dog;
    animal.speak();
    Formatter formatter = Formatter();
    formatter.format(1);
    formatter.format(\"ok\");
}
",
    );
    assert!(accepted.is_empty(), "{accepted}");

    let rejected = messages(
        "\
abstract class Animal { @abstract void speak(); }
class Dog extends Animal { void speak() {} }
class Cat extends Animal {}
abstract class Shape {}
class Bad extends Shape { @override void missing() {} }
@final class Closed {}
class Open extends Closed {}
class Utility {
    @static @override void run() {}
}
class Parent { @final void lock() {} }
class Child extends Parent { @override void lock() {} }
interface Named { str label(); }
class Item implements Named {}
void main() {
    Animal animal = new Animal();
    Shape shape = Shape();
}
",
    );
    assert!(rejected.contains("needs @override"), "{rejected}");
    assert!(rejected.contains("must implement 'speak'"), "{rejected}");
    assert!(rejected.contains("does not override"), "{rejected}");
    assert!(rejected.contains("final class"), "{rejected}");
    assert!(
        rejected.contains("@static and @override cannot be combined"),
        "{rejected}"
    );
    assert!(rejected.contains("final method"), "{rejected}");
    assert!(
        rejected.contains("must implement 'Named.label'"),
        "{rejected}"
    );
    assert!(
        rejected.contains("cannot construct abstract class 'Animal'"),
        "{rejected}"
    );
    assert!(
        rejected.contains("cannot construct abstract class 'Shape'"),
        "{rejected}"
    );
}

#[test]
fn generators_and_iterators_typecheck() {
    let accepted = messages(
        "\
iter<T> repeat<T>(T value, int count) {
    loop (int i = 0; i < count; i++) {
        emit value;
    }
}

void main() {
    iter<int> numbers = repeat<int>(1, 2);
    iter<str> words = repeat<str>(\"hi\", 1);
    loop (int n : numbers) {}
    loop (chr c : \"ab\") {}
    iter<int> r = range(0, 5, 1);
}
",
    );
    assert!(accepted.is_empty(), "{accepted}");

    let missing_args = messages(
        "\
iter<T> repeat<T>(T value, int count) {
    emit value;
}
void main() {
    iter<int> n = repeat(1, 2);
}
",
    );
    assert!(
        missing_args.contains("requires type arguments"),
        "{missing_args}"
    );

    let bad_emit = messages(
        "\
iter<int> bad() {
    emit \"nope\";
}
void main() {}
",
    );
    assert!(bad_emit.contains("expected 'int64'"), "{bad_emit}");

    let valued_return = messages(
        "\
iter<int> bad() {
    emit 1;
    return 2;
}
void main() {}
",
    );
    assert!(
        valued_return.contains("cannot return a value"),
        "{valued_return}"
    );

    let emit_outside = messages(
        "\
void bad() {
    emit 1;
}
void main() {}
",
    );
    assert!(
        emit_outside.contains("emit is only allowed"),
        "{emit_outside}"
    );

    let zero_step = messages(
        "\
void main() {
    iter<int> it = range(0, 5, 0);
}
",
    );
    assert!(zero_step.contains("step cannot be zero"), "{zero_step}");

    let optional_generics = messages(
        "\
T? empty<T>() {
    return null;
}
iter<T>? missing<T>() {
    return null;
}
void main() {
    int? a = empty<int>();
    iter<int>? it = missing<int>();
}
",
    );
    assert!(optional_generics.is_empty(), "{optional_generics}");
}
