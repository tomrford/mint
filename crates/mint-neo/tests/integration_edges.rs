use mint_neo::{Category, Source, compile_header};

fn header(name: &str, text: &str) -> Source {
    Source::new(name, text)
}

fn compile(name: &str, text: &str) -> Result<mint_neo::CompiledSchema, mint_neo::Error> {
    compile_header(header(name, text))
}

fn compile_err(name: &str, text: &str) -> mint_neo::Error {
    compile(name, text).expect_err("expected a schema diagnostic")
}

fn render(error: &mint_neo::Error) -> String {
    error.render(&[])
}

fn spanned_text<'a>(error: &mint_neo::Error, text: &'a str) -> &'a str {
    let span = error.diagnostics[0].span.expect("diagnostic span");
    &text[span.start..span.end]
}

#[test]
fn unused_function_like_macros_are_accepted_as_trivia() {
    let schema = compile(
        "macros.h",
        r#"
#include <stdint.h>
#define WIDTH(x) (x)
#define PAIR(a, b) ((a) + (b))
#define N 2u
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct {
    uint16_t values[N];
} config_t;
"#,
    )
    .expect("unused function-like macros are trivia");
    assert_eq!(schema.layout.root_layout().size, 4);
}

#[test]
fn function_like_macro_used_as_array_extent_is_rejected() {
    let error = compile_err(
        "macros.h",
        r#"
#include <stdint.h>
#define WIDTH(x) (x)
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct {
    uint16_t samples[WIDTH(4)];
} config_t;
"#,
    );
    let rendered = render(&error);
    assert!(rendered.contains("function-like macro"), "{rendered}");
    assert!(rendered.contains("macros.h:"), "{rendered}");
    assert!(rendered.contains("WIDTH"), "{rendered}");
}

#[test]
fn oversized_root_diagnostic_uses_header_source_and_root_span() {
    let text = r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0
 */
typedef struct {
    uint32_t values[67108865];
} config_t;
"#;
    let error = compile_err("root-size.h", text);
    assert_eq!(error.diagnostics[0].category, Category::Schema);
    assert_eq!(error.diagnostics[0].source, "root-size.h");
    assert!(
        spanned_text(&error, text).contains("typedef struct"),
        "root span must cover the root typedef"
    );

    let rendered = render(&error);
    assert!(
        rendered.contains("resolved root size") && rendered.contains("256 MiB"),
        "{rendered}"
    );
    assert!(rendered.contains(" --> root-size.h:"), "{rendered}");
    assert!(
        rendered.contains("typedef struct {"),
        "root-size diagnostic must excerpt the header, got:\n{rendered}"
    );
    assert!(rendered.contains("^"), "{rendered}");
    assert!(
        !rendered.contains(" --> config_t:"),
        "must not use the root type name as the diagnostic source:\n{rendered}"
    );
}

#[test]
fn misaligned_start_address_reports_target_units_and_annotation_span() {
    let text = r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi ti-c28x-eabi
 * @mint start-address 0x1
 */
typedef struct {
    uint32_t id;
} config_t;
"#;
    let error = compile_err("c28x-align.h", text);
    assert_eq!(error.diagnostics[0].category, Category::Schema);
    assert_eq!(error.diagnostics[0].source, "c28x-align.h");
    assert!(
        spanned_text(&error, text).contains("@mint start-address 0x1"),
        "start-address span must cover the annotation"
    );

    let rendered = render(&error);
    assert!(
        rendered.contains("start-address 0x1 is not aligned"),
        "must report the annotated target-unit start-address, not the converted octet address:\n{rendered}"
    );
    assert!(
        !rendered.contains("start-address 0x2"),
        "must not substitute the C28x octet address 0x2:\n{rendered}"
    );
    assert!(rendered.contains(" --> c28x-align.h:"), "{rendered}");
    assert!(rendered.contains("/**"), "{rendered}");
    assert!(rendered.contains("^"), "{rendered}");
    assert!(
        !rendered.contains(" --> config_t:"),
        "must not use the root type name as the diagnostic source:\n{rendered}"
    );
}

#[test]
fn octet_address_overflow_uses_header_source_and_start_address_span() {
    let text = r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi ti-c28x-eabi
 * @mint start-address 0x80000000
 */
typedef struct {
    uint16_t id;
} config_t;
"#;
    let error = compile_err("octet-overflow.h", text);
    assert_eq!(error.diagnostics[0].category, Category::Encoding);
    assert_eq!(error.diagnostics[0].source, "octet-overflow.h");
    assert!(
        spanned_text(&error, text).contains("@mint start-address 0x80000000"),
        "octet-address overflow span must cover the annotation"
    );

    let rendered = render(&error);
    assert!(
        rendered.contains("start-address cannot be represented as a 32-bit octet address"),
        "{rendered}"
    );
    assert!(rendered.contains(" --> octet-overflow.h:"), "{rendered}");
    assert!(rendered.contains("/**"), "{rendered}");
    assert!(rendered.contains("^"), "{rendered}");
    assert!(
        !rendered.contains(" --> config_t:") && !rendered.contains(" --> header:"),
        "must render against the header source, got:\n{rendered}"
    );
}

#[test]
fn output_range_overflow_uses_header_source_and_start_address_span() {
    let text = r#"
#include <stdint.h>
/**
 * @mint block
 * @mint abi generic-le
 * @mint start-address 0xFFFFFFF0
 */
typedef struct {
    uint8_t bytes[32];
} config_t;
"#;
    let error = compile_err("range-overflow.h", text);
    assert_eq!(error.diagnostics[0].category, Category::Encoding);
    assert_eq!(error.diagnostics[0].source, "range-overflow.h");
    assert!(
        spanned_text(&error, text).contains("@mint start-address 0xFFFFFFF0"),
        "range overflow span must cover the annotation"
    );

    let rendered = render(&error);
    assert!(
        rendered.contains("exceeds the 32-bit address space"),
        "{rendered}"
    );
    assert!(rendered.contains(" --> range-overflow.h:"), "{rendered}");
    assert!(rendered.contains("/**"), "{rendered}");
    assert!(rendered.contains("^"), "{rendered}");
    assert!(
        !rendered.contains(" --> config_t:"),
        "must not use the root type name as the diagnostic source:\n{rendered}"
    );
}
