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

    let rejected = messages("void main() {\n    auto chr value = true;\n}\n");
    assert!(rejected.contains("not supported"), "{rejected}");
}

#[test]
fn short_circuit_is_not_emitted_as_valid() {
    let text = messages("void main() {\n    bln value = true && false;\n}\n");
    assert!(text.contains("short-circuit"), "{text}");
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
    assert!(mixed.contains("same integer type"), "{mixed}");

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
    assert!(calls.contains("unknown name 'add'"), "{calls}");
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

    let unsupported = messages(
        "\
void main() {
    chr mark = chr(\"A\");
    bln flag = bln(mark);
    chr other = chr(true);
}
",
    );
    assert!(
        unsupported.contains("cannot convert 'chr' to 'bln'"),
        "{unsupported}"
    );
    assert!(
        unsupported.contains("cannot convert 'bln' to 'chr'"),
        "{unsupported}"
    );

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
    loop (int value = values.iter()) {
        io.println(value);
    }
    loop (uint64 idx, int value = values.enumerate()) {
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
    let outside = messages("void main() {\n    int[] values = [1];\n    values.iter();\n}\n");
    assert!(
        outside.contains("can only be used as a loop source"),
        "{outside}"
    );
    let index_type = messages(
        "void main() {\n    int[] values = [1];\n    loop (int idx, int value = values.enumerate()) {\n        io.println(value);\n    }\n}\n",
    );
    assert!(index_type.contains("uint64"), "{index_type}");
    let constant = messages(
        "void main() {\n    const int[] values = [1];\n    values[0] = 2;\n    values.push(3);\n}\n",
    );
    assert!(constant.contains("cannot assign to constant"), "{constant}");
    let during = messages(
        "void main() {\n    int[] values = [1];\n    loop (int value = values.iter()) {\n        values.push(value);\n    }\n}\n",
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
    let different =
        messages("void main() {\n    int[] dynamic = [1];\n    int[3] bounded = dynamic;\n}\n");
    assert!(different.contains("type mismatch"), "{different}");
    let widening =
        messages("void main() {\n    int[3] bounded = [1];\n    int[] dynamic = bounded;\n}\n");
    assert!(widening.contains("type mismatch"), "{widening}");
    let literal_count = messages("void main() {\n    int a, int b = [1];\n}\n");
    assert!(
        literal_count.contains("expects 2 elements, found 1"),
        "{literal_count}"
    );
    let too_wide = messages("void main() {\n    int32[] values = [4000000000];\n}\n");
    assert!(too_wide.contains("does not fit"), "{too_wide}");
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
