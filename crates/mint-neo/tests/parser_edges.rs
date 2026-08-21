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
fn trailing_comments_on_accepted_directives_are_trivia() {
    let schema =
        compile(&mint_block("", "typedef struct { uint32_t id; } config_t;")).expect("header");
    assert_eq!(schema.layout.root_layout().size, 4);
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
fn macro_and_enumerator_name_collision_is_rejected() {
    let error = compile_err(&mint_block(
        r#"
#define AXIS_COUNT 3u
typedef enum { AXIS_COUNT = 4 } dimensions_t;
"#,
        "typedef struct { uint16_t axes[AXIS_COUNT]; } config_t;",
    ));
    assert!(
        error.contains("enumerator") || error.contains("macro"),
        "{error}"
    );
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
fn leading_mint_attaches_through_ordinary_and_doxygen_comments() {
    let schema = compile(
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
    )
    .expect("intervening comments");
    assert_eq!(schema.layout.root_layout().size, 4);
}

#[test]
fn leading_slash_slash_slash_attaches_through_ordinary_comment() {
    let schema = compile(
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
    )
    .expect("ordinary comment");
    assert_eq!(schema.layout.root_layout().size, 4);
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
fn acyclic_macro_expansion_is_bounded() {
    let mut text = String::from("#include <stdint.h>\n#define M0 1u\n");
    for index in 1..=200 {
        text.push_str(&format!("#define M{index} M{}\n", index - 1));
    }
    text.push_str(
        r#"
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct { uint16_t values[M200]; } config_t;
"#,
    );
    let error = compile_err(&text);
    assert!(error.contains("exceeds"), "{error}");
}
