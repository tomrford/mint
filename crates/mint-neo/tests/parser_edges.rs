use mint_neo::{Source, compile_header};

fn header(text: &str) -> Source {
    Source::new("config.h", text)
}

fn compile(text: &str) -> Result<mint_neo::CompiledSchema, mint_neo::Error> {
    compile_header(header(text))
}

fn compile_err(text: &str) -> String {
    compile(text)
        .expect_err("expected a schema diagnostic")
        .to_string()
}

fn mint_block(prelude: &str, root: &str) -> String {
    let prelude = prelude.trim();
    let root = root.trim();
    if prelude.is_empty() {
        format!(
            "#pragma once // guard\n#include <stdint.h> /* types */\n/**\n * @mint block\n * @mint abi generic-le\n * @mint start-address 0\n */\n{root}\n"
        )
    } else {
        format!(
            "#pragma once // guard\n#include <stdint.h> /* types */\n{prelude}\n/**\n * @mint block\n * @mint abi generic-le\n * @mint start-address 0\n */\n{root}\n"
        )
    }
}

#[test]
fn object_like_macro_bodies_strip_c_comments() {
    let schema = compile(&mint_block(
        r#"
#define CHANNELS 4u /* count */
#define WIDTH (2u /* x */ + 2u) // cols
"#,
        r#"
typedef struct {
    uint16_t samples[CHANNELS];
    uint16_t row[WIDTH];
} config_t;
"#,
    ))
    .expect("header");
    assert_eq!(schema.layout.root_layout().size, 16);
}

#[test]
fn referenced_duplicate_macros_are_rejected() {
    let error = compile_err(&mint_block(
        r#"
#define N 1u
#define N 2u
"#,
        "typedef struct { uint16_t values[N]; } config_t;",
    ));
    assert!(error.contains("duplicate"), "{error}");
}

#[test]
fn unreferenced_duplicate_macros_are_ignored() {
    let schema = compile(&mint_block(
        r#"
#define UNUSED 1u
#define UNUSED 2u
#define N 3u
"#,
        "typedef struct { uint16_t values[N]; } config_t;",
    ))
    .expect("unreferenced duplicates");
    assert_eq!(schema.layout.root_layout().size, 6);
}

#[test]
fn nested_reusable_struct_tags_are_discovered() {
    let schema = compile(&mint_block(
        "",
        r#"
typedef struct {
    struct point {
        uint16_t x;
        uint16_t y;
    } origin;
    struct point dest;
} config_t;
"#,
    ))
    .expect("nested tag");
    assert_eq!(schema.layout.root_layout().size, 8);
}

#[test]
fn leading_mint_attaches_through_intervening_comments() {
    let cases = [
        (
            "block",
            r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
/* local working copy */
/**
 * Additional documentation.
 */
typedef struct {
    uint32_t id;
} config_t;
"#,
        ),
        (
            "slash-slash-slash",
            r#"
#include <stdint.h>
/// @mint block
/// @mint abi generic-le
/// @mint start-address 0
// keep this copy
typedef struct {
    uint32_t id;
} config_t;
"#,
        ),
    ];
    for (name, source) in cases {
        let schema = compile(source).expect(name);
        assert_eq!(schema.layout.root_layout().size, 4, "{name}");
    }
}

#[test]
fn blank_line_still_detaches_leading_mint() {
    let error = compile_err(
        r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */

typedef struct { uint32_t id; } config_t;
"#,
    );
    assert!(
        error.contains("attach") || error.contains("block"),
        "{error}"
    );
}

#[test]
fn mint_tags_in_invalid_locations_are_rejected() {
    let fingerprint_on_typedef = compile_err(
        r#"
#include <stdint.h>
/// @mint fingerprint
typedef uint64_t id_t;
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct { id_t id; } config_t;
"#,
    );
    assert!(
        fingerprint_on_typedef.contains("fingerprint"),
        "{fingerprint_on_typedef}"
    );

    let block_on_field = compile_err(&mint_block(
        "",
        r#"
typedef struct {
    /// @mint abi generic-le
    uint32_t id;
} config_t;
"#,
    ));
    assert!(
        block_on_field.contains("block metadata") || block_on_field.contains("root"),
        "{block_on_field}"
    );

    let fingerprint_on_unreachable_field = compile_err(
        r#"
#include <stdint.h>
typedef struct {
    uint64_t ignored; /**< @mint fingerprint */
} helper_t;
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct { uint32_t id; } config_t;
"#,
    );
    assert!(
        fingerprint_on_unreachable_field.contains("direct member of the root"),
        "{fingerprint_on_unreachable_field}"
    );
}

#[test]
fn ordinary_multi_declarator_typedefs_resolve_per_name() {
    let schema = compile(&mint_block(
        r#"
typedef uint32_t id_t, count_t;
typedef uint16_t row_t[4], pair_t[2];
"#,
        r#"
typedef struct {
    id_t id;
    count_t count;
    row_t row;
    pair_t pair;
} config_t;
"#,
    ))
    .expect("multi-declarator aliases");
    assert_eq!(schema.layout.root_layout().size, 20);
}

#[test]
fn reachable_types_follow_c_declaration_order_and_namespaces() {
    let later_typedef = compile_err(
        r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct { later_t value; } config_t;
typedef uint32_t later_t;
"#,
    );
    assert!(
        later_typedef.contains("not declared before this use"),
        "{later_typedef}"
    );

    let later_struct_definition = compile_err(
        r#"
#include <stdint.h>
struct item;
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct { struct item value; } config_t;
struct item { uint32_t id; };
"#,
    );
    assert!(
        later_struct_definition.contains("incomplete at this use"),
        "{later_struct_definition}"
    );

    let bare_struct_tag = compile_err(&mint_block(
        "struct item { uint32_t id; };\n",
        "typedef struct { item value; } config_t;",
    ));
    assert!(
        bare_struct_tag.contains("unknown type 'item'"),
        "{bare_struct_tag}"
    );

    compile(&mint_block(
        r#"
typedef struct item item_t;
struct item { uint32_t id; };
"#,
        "typedef struct { item_t value; } config_t;",
    ))
    .expect("a forward-declared tag completed before use is valid C");
}

#[test]
fn annotated_multi_declarator_typedef_is_rejected() {
    let error = compile_err(
        r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct { uint32_t id; } first_t, second_t;
"#,
    );
    assert!(error.contains("exactly one name"), "{error}");
}

#[test]
fn unreachable_packed_helper_is_trivia() {
    let schema = compile(&mint_block(
        "typedef struct { uint8_t a; uint32_t b; } unused_t __attribute__((packed));\n",
        "typedef struct { uint32_t id; } config_t;",
    ))
    .expect("unreachable packed helper");
    assert_eq!(schema.layout.root_layout().size, 4);
}

#[test]
fn duplicate_member_names_report_the_previous_span() {
    let error = compile_err(&mint_block(
        "",
        r#"
typedef struct {
    uint32_t id;
    uint16_t id;
} config_t;
"#,
    ));
    assert!(error.contains("duplicate member"), "{error}");
    assert!(error.contains("previous member is here"), "{error}");
}

#[test]
fn flattened_array_dimension_limit_uses_declarator_span() {
    let text = mint_block(
        "typedef uint8_t t10_t[2][2][2][2][2][2][2][2][2][2];\n",
        r#"
typedef struct {
    t10_t grid[2][2][2][2][2][2][2];
} config_t;
"#,
    );
    let error = compile(&text).expect_err("dimension limit");
    let span = error.diagnostic.span.expect("span");
    assert!(
        span.end > span.start && span.start > 0,
        "flattened dimension overflow must use a real declarator span, got {span:?}"
    );
    let excerpt = &text[span.start..span.end];
    assert!(
        excerpt.contains("grid"),
        "span must cover the overflowing declarator, got {excerpt:?}"
    );
    assert!(!error.to_string().contains(" --> config.h:1:1"), "{error}");
    assert!(
        error.to_string().contains("at most 16 dimensions"),
        "{error}"
    );
}

#[test]
fn malformed_object_like_defines_are_fatal() {
    let cases = [
        (
            "garbage after comment",
            "#define FOO 1 /* c */ @@@\n#define N 2u\n",
        ),
        ("invalid define name", "#define 1 2\n#define N 2u\n"),
        ("empty define", "#define\n#define N 2u\n"),
    ];
    for (name, prelude) in cases {
        let error = compile_err(&mint_block(
            prelude,
            "typedef struct { uint16_t values[N]; } config_t;",
        ));
        assert!(
            error.contains("invalid C syntax"),
            "{name}: expected invalid C syntax in {error}"
        );
    }
}

#[test]
fn acyclic_typedef_alias_chain_is_bounded() {
    let mut text = String::from("#include <stdint.h>\ntypedef uint32_t t0;\n");
    for index in 1..=200 {
        text.push_str(&format!("typedef t{} t{};\n", index - 1, index));
    }
    text.push_str(
        r#"
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct { t200 value; } config_t;
"#,
    );
    let error = compile_err(&text);
    assert!(error.contains("exceeds"), "{error}");
}

#[test]
fn macros_preserve_c_precedence_and_enum_declaration_context() {
    let text = mint_block(
        "#define N 1 + \\\n2\nenum { FIRST = N * 2, NEXT, UNUSED = -1, RESET = 7, LAST };\n#define LATER 99\n",
        "typedef struct { uint16_t values[N * 2]; uint16_t other[NEXT]; uint16_t last[LAST]; } config_t;",
    );
    let schema = compile_header(Source::new("config.h", text)).unwrap();
    let fields = &schema.layout.root_layout().fields;
    assert_eq!(
        fields.iter().map(|f| f.size).collect::<Vec<_>>(),
        [10, 12, 16]
    );
    let late = mint_block(
        "#define N LATER\nenum { COUNT = N };\n#define LATER 3\n",
        "typedef struct { uint16_t values[COUNT]; } config_t;",
    );
    assert!(compile_err(&late).contains("not available"));
}

#[test]
fn reachable_macro_rewrites_and_conflicting_builtin_typedefs_are_rejected() {
    for (prelude, member) in [
        (
            "typedef uint32_t word_t;\n#define word_t uint64_t\n",
            "word_t value;",
        ),
        ("#define value renamed\n", "uint32_t value;"),
        ("#define uint32_t uint64_t\n", "uint32_t value;"),
        ("typedef uint64_t float32_t;\n", "float32_t value;"),
        ("typedef uint64_t uint32_t;\n", "uint32_t value;"),
        ("typedef uint32_t int32_t;\n", "int32_t value;"),
    ] {
        let error = compile_err(&mint_block(
            prelude,
            &format!("typedef struct {{ {member} }} config_t;"),
        ));
        assert!(
            error.contains("macro") || error.contains("conflicts"),
            "{error}"
        );
    }
    compile_header(Source::new(
        "config.h",
        mint_block(
            "typedef float float32_t;\ntypedef double float64_t;\n",
            "typedef struct { float32_t x; float64_t y; } config_t;",
        ),
    ))
    .unwrap();
}

#[test]
fn unreachable_local_types_and_enum_expressions_do_not_change_the_schema() {
    let plain = mint_block(
        "typedef uint32_t word_t;\n",
        "typedef struct { word_t value; } config_t;",
    );
    let extra = mint_block(
        "typedef uint32_t word_t;\nenum { ERROR = -1, MASK = 1 << 3 };\nvoid f(void) { typedef uint16_t word_t; enum { COUNT = -1 }; }\nvoid g(void) { typedef uint8_t word_t; enum { COUNT = 1 << 4 }; }\n",
        "typedef struct { word_t value; } config_t;",
    );
    let a = compile_header(Source::new("a.h", plain)).unwrap();
    let b = compile_header(Source::new("b.h", extra)).unwrap();
    assert_eq!(a.fingerprint, b.fingerprint);
    assert_eq!(b.layout.root_layout().size, 4);
}

#[test]
fn record_depth_is_independent_of_cached_shallow_fields() {
    for wrappers in [126, 127] {
        let mut prelude = String::from("typedef struct { uint16_t x; } leaf_t;\n");
        let mut previous = String::from("leaf_t");
        for n in 0..wrappers {
            prelude.push_str(&format!("typedef struct {{ {previous} child; }} r{n}_t;\n"));
            previous = format!("r{n}_t");
        }
        for shallow in ["", "leaf_t shallow;"] {
            let text = mint_block(
                &prelude,
                &format!("typedef struct {{ {shallow} {previous} deep; }} config_t;"),
            );
            let result = compile_header(Source::new("config.h", text));
            if wrappers == 126 {
                assert!(result.is_ok(), "{result:?}");
            } else {
                assert!(result.unwrap_err().to_string().contains("record nesting"));
            }
        }
    }
}

#[test]
fn expansion_work_and_expression_nesting_are_bounded() {
    let mut prelude = String::from("#define N0 1\n");
    for n in 1..29 {
        prelude.push_str(&format!("#define N{n} (N{} + N{})\n", n - 1, n - 1));
    }
    let text = mint_block(
        &prelude,
        "typedef struct { uint16_t values[N28]; } config_t;",
    );
    assert!(compile_err(&text).contains("exceeds"));
    let prelude = format!("#define N {}1{}\n", "(".repeat(1000), ")".repeat(1000));
    assert!(
        compile_err(&mint_block(
            &prelude,
            "typedef struct { uint16_t values[N]; } config_t;"
        ))
        .contains("nesting exceeds")
    );
    assert!(
        compile_err(&mint_block(
            "#define N ++1\n",
            "typedef struct { uint16_t values[N]; } config_t;"
        ))
        .contains("increment")
    );
}
