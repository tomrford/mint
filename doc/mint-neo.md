# Mint Neo design specification

## Status

This document proposes a separate tool with the working name **Mint Neo**. It is
not a Mint v3 migration plan. Mint v2 remains the full-featured TOML and
Excel/JSON tool.

Mint Neo is a smaller alternative for projects that already prepare their data
before image generation:

```text
one self-contained C header + one resolved JSON object -> one Intel HEX range
```

The target experience is to describe the firmware-visible shape once, in C,
and fill that shape without maintaining a parallel layout file.

## Product position

Mint v2 is the broad tool. It owns a layout language, supports several data
workflows and performs some image post-processing.

Mint Neo is the narrow tool. It follows one C aggregate, requires exact input
data and delegates preparation and post-processing to other tools. It should
be easier to understand because it offers fewer policies:

- An upstream script, `jq` or another data tool prepares one final JSON object.
- Mint Neo encodes that object according to one header and one named ABI.
- Hexy, hexview or another downstream tool pads, combines, checksums, signs or
  converts the image.

## Goals

- Make one firmware header the only layout/schema file.
- Accept one already-resolved JSON object as the only data source.
- Reuse Mint v2's named ABI rules and confidence in their scalar and aggregate
  layouts.
- Support reusable local typedefs, nested records and fixed arrays of records.
- Require the JSON shape and every array extent to match the C shape exactly.
- Preserve deterministic, nameless ABI fingerprints.
- Produce one standard, octet-addressed Intel HEX range.
- Produce precise source diagnostics for both the header and JSON.

## Non-goals

- Parsing arbitrary production headers.
- Following user includes or conditional compilation.
- Running a compiler, preprocessor or linker.
- Providing a complete C preprocessor or constant-expression evaluator.
- Merging variants, versions, defaults or overlays.
- Excel support.
- Checksums, signatures or other post-processing.
- Pointer or linker relocation semantics.
- Address references between fields or blocks.
- Allocated-region management or end-of-block padding.
- Multiple blocks or multiple output ranges in one invocation.
- Motorola S-record, raw binary or vendor-specific HEX output.
- Generating, rewriting or emitting C code of any kind.
- Providing a stable library API or language binding in the first version.
- Maintaining command-line compatibility with Mint v2.

## Transformation from Mint v2

### Kept

| Mint v2 capability | Mint Neo form |
| --- | --- |
| Named ABI profiles | A required `@mint abi` tag on the root record |
| Scalar byte order, size and alignment | Applied by the same effective ABI rules |
| ABI aggregate alignment and tail padding | Applied recursively to C records and arrays |
| Configurable alignment-padding byte | Optional `@mint padding`; default `0xFF` |
| Block start address | Required `@mint start-address` |
| Strict integer conversion | The only integer conversion mode |
| ABI fingerprint calculation | New Neo fingerprint domain and CLI operation |
| Optional stored fingerprint field | `@mint fingerprint` on one `uint64_t` field |
| Intel HEX rendering | The only output format, with fixed record policy |
| ABI discovery | `abi list` and `abi show` remain available |
| Resolved-layout diagnostics | Reintroduced as `inspect` |
| 32-bit octet output-address validation | Retained |

### Removed

| Mint v2 capability | Replacement or reason |
| --- | --- |
| TOML layouts | The C header is the schema |
| Generated headers | Firmware consumes the input header directly |
| Excel | Prepare JSON upstream |
| Variants and `/` fallback | Merge and validate upstream, then pass one final object |
| Literal `value` sources | Put the value in JSON |
| Named source aliases | JSON keys must match C member names |
| `[mint.const]` payload values | Put the value in JSON; firmware can check it against its own macro |
| Lowercase `size` padding | All dimensions are declared by C and inputs must match exactly |
| Uppercase `SIZE` | Exact size is the only policy |
| Bitmaps | Pack the integer upstream |
| Fixed-point conversion | Encode the storage integer upstream |
| Refs and reflists | Pass an ordinary integer address in JSON if the schema stores one |
| C pointer fields | Rejected because the modeled ABIs do not define pointer representation |
| Checksums | Apply them downstream |
| Named checksum configurations | No checksum engine exists in Neo |
| Allocated block `length` | Output length is exactly the root's resolved size in octets |
| Padding to allocated length | Downstream responsibility |
| Multiple blocks and overlap checks | One root record and one output range |
| Multi-file selectors | One header per invocation |
| Motorola S-record output | Convert downstream when needed |
| Output format selection | Intel HEX only |
| Record-width selection | One stable built-in record width |
| Used-values export | The input JSON is already the resolved value report |
| `--strict` | Strictness is unconditional |
| `--stats` | Use `inspect` for layout information |
| Header/fingerprint target references | One independent root fingerprint |
| Layout-driven string padding | No implicit string-to-array conversion in the first version |

### Gained

- The schema is valid C and can be included by firmware.
- Named local typedefs can be reused.
- Records can contain records through named or unnamed struct types.
- Fixed arrays can contain scalars, records or further fixed arrays.
- Arrays can have arbitrary fixed dimensions within explicit resource limits.
- JSON structurally mirrors records and arrays instead of using flat lookup
  names.
- All missing, extra, mistyped and incorrectly sized JSON values are errors.
- Header diagnostics identify exact source spans and related declarations.

## Settings scope

Mint v2 has three file-level layout settings:

| Mint v2 file-level setting | Mint Neo decision |
| --- | --- |
| `abi` | Retained on the one root block |
| Named checksum configurations | Removed with checksums |
| `[mint.const]` | Removed with layout-supplied payload values |

Neo therefore has no file-level metadata comment. Its complete configuration
belongs to the root block annotation:

- `abi` defines scalar, aggregate and address-unit behaviour;
- `start-address` defines placement; and
- `padding` defines bytes used only for ABI-required alignment gaps.

With exactly one block per header, separating ABI into a file comment would
create a second annotation location without adding scope. If multiple blocks
are ever supported, each block should repeat its own complete settings rather
than introduce implicit global inheritance.

## One-file rule

Mint Neo reads one file and never opens a referenced header.

All user-defined types reachable from the root record must have complete
definitions in that file. Standard fixed-width scalar names are built into
Mint Neo; an optional `#include <stdint.h>` remains in the file for the C
compiler but is not opened by Neo.

The first version accepts exactly these preprocessor forms anywhere in the
file:

- `#pragma once`, treated as trivia;
- `#include <stdint.h>`, `#include <stdfloat.h>`, `#include <stddef.h>` and
  `#include <stdbool.h>`, treated as trivia; and
- macro definitions, which are ignored unless a reachable array extent names
  them.

Every other directive is rejected unconditionally. This includes every other
include, every conditional directive, `#undef`, `#error`, `#line`, `_Pragma`
and every other pragma. Reachability is not consulted because Neo cannot know
whether an unsupported directive changes a reachable declaration.

This restriction is ABI-relevant, not merely parser simplification. Pragmas
such as `#pragma pack`, vendor alignment controls, scalar-storage-order controls
and compiler mode controls can change field offsets or representations.
`#pragma once` is the sole accepted pragma because it only controls repeated
inclusion and cannot change the root shape.

Only object-like macros referenced by reachable shape expressions must satisfy
Neo's shape-expression grammar. Macro replacement preserves C token precedence;
it does not implicitly parenthesise the replacement list. Macros that rewrite
reachable type names, member names or other declaration tokens are rejected.
Unreferenced macro definitions are ignored. Referring to a function-like macro
from an extent is an error.

Declarations not reachable from the root record are parsed for syntax but are
otherwise ignored. Function-local and prototype-local declarations do not enter
the file-scope type or constant tables. Enum expressions are evaluated only
when needed by a reachable extent, in their original declaration context. An unreachable pointer, union, bitfield, function prototype
or external object does not become part of the schema.

## Header annotations

Mint metadata uses strict Doxygen-style comments. Neo recognises only `@mint`
tags and leaves other documentation text untouched.

The block annotation immediately precedes the one root record:

```c
/**
 * Configuration persisted in flash.
 *
 * @mint block
 * @mint abi arm-aapcs32-le
 * @mint start-address 0x8000
 * @mint padding 0xFF
 */
typedef struct {
    uint64_t fingerprint; /**< @mint fingerprint */
    uint32_t device_id;
    float gain;
} config_t;
```

Rules:

- Exactly one complete record typedef has `@mint block`, and that typedef
  introduces exactly one root type name.
- `@mint abi` and `@mint start-address` are required.
- `@mint padding` is optional and defaults to `0xFF`.
- Block metadata may appear only in the root record's leading Doxygen comment.
- `@mint fingerprint` may appear on at most one direct member of the root
  record. That member must resolve to exactly `uint64_t` and must not be an
  array.
- A leading annotation attaches to the declaration beginning at the next
  non-comment token only when no blank line separates them.
- A trailing `/**< ... */` annotation attaches to the preceding member only
  when it begins on the same line as that member's terminating semicolon.
- An `@mint` comment that satisfies neither attachment rule is an error.
- An annotated member declaration must contain exactly one declarator, and an
  annotated typedef must introduce exactly one name.
- Unknown `@mint` subtags, duplicate `@mint` tags and `@mint` tags in invalid
  locations are errors. Other Doxygen commands are ignored.
- Ordinary comments never affect encoding.

There are deliberately no tags for names, values, dimensions, refs, checksums,
bitmaps, fixed-point formats or output options.

Tags are case-sensitive and use `@mint`, not `\mint`. `/** ... */`,
`/*! ... */`, contiguous `///` lines and trailing `/**< ... */` are accepted.
The parser removes normal Doxygen comment decoration, then reads one `@mint`
tag per logical line. Text after a tag's expected value is an error.

Metadata integers use C decimal, hexadecimal or octal syntax with optional
unsigned suffixes. `start-address` is an unsigned 32-bit value expressed in
the selected ABI's addressable units. `padding` is one unsigned octet.

The start address remains in the header because Neo deliberately models one
deployable block per schema. It does not contribute to the fingerprint, which
continues to identify ABI shape rather than placement.

## C schema dialect

Mint Neo uses a C syntax parser but accepts a closed semantic subset. Valid C
outside this subset is not automatically valid Neo schema.

### Scalar types

The first version supports:

- `uint8_t`, `uint16_t`, `uint32_t`, `uint64_t`;
- `int8_t`, `int16_t`, `int32_t`, `int64_t`;
- `float`, `double`, `float32_t` and `float64_t`; and
- local typedef aliases that eventually resolve to one of these types.

A local typedef using a built-in scalar name must resolve to that same scalar
representation; conflicting definitions are errors.

`float32_t` and `float64_t` explicitly select IEEE-754 binary32 and binary64.
`float` and `double` map to those same representations on all current profiles,
including TI C28x EABI. All four use the selected ABI's byte order. ABI
verification must test format as well as size and alignment.

The first version rejects `_Bool`, `bool`, plain `char`, `short`, `int`,
`long`, `long long`, `size_t`, enums as stored fields and all compiler-specific
scalar types.

Supporting native C integer spellings later requires extending every applicable
ABI profile with their storage width, alignment and signedness rules. Plain
`char` also requires a signedness policy. These types must not be inferred from
the machine running Neo.

### Qualifiers

`const` and `volatile` do not change persisted representation and are ignored
while resolving reachable object fields. `_Atomic`, `restrict`, address-space
qualifiers and compiler attributes are rejected.

### Records and aliases

Supported declarations include:

```c
typedef uint32_t counter_t;

typedef struct {
    uint16_t x;
    uint16_t y;
} point_t;

typedef struct table {
    point_t points[4];
    counter_t generation;
} table_t;
```

The resolver:

1. collects record tags and typedef names;
2. assigns each declaration a stable internal type identity;
3. resolves aliases;
4. builds dependencies between records;
5. rejects incomplete by-value members and dependency cycles; and
6. computes each reachable type once.

Typedef and field names do not contribute to the ABI fingerprint.

The root and every reachable record must have at least one named member.
An unnamed struct type used by a named member is supported. C11 anonymous
members, where an embedded record has no member name, are rejected because
they have no unambiguous JSON property.

Every reachable member declaration introduces exactly one member. Declarations
such as `uint32_t a, b;` are rejected so annotations, spans and diagnostics
always have one declaration target.

### Arrays

Every array dimension is part of the C declarator:

```c
uint16_t samples[4];
uint16_t matrix[3][4];
point_t trajectories[2][8][16];
```

The language has no special one-dimensional or two-dimensional cases. The
implementation accepts up to 16 fixed dimensions. Every extent must resolve to
a positive compile-time integer and the complete root must remain within the
256 MiB resolved-size limit.

Arrays remain first-class nodes in the semantic and fingerprint IR:

```text
array
  element type: resolved scalar or record
  dimensions: ordered list, for example [2, 8, 16]
  stride: resolved element size including tail padding
```

Neo must not unroll arrays of records into synthetic named fields. It resolves
one element layout and uses its tail-padded `sizeof` as the next dimension's
stride.

Before layout and hashing, aliases are replaced by their targets and chained
array declarators are canonicalised. For example,
`typedef uint16_t row_t[4]; row_t grid[3];` and
`uint16_t grid[3][4];` resolve to the same array type.

### Array extent constants

Requiring literal extents only would make normal firmware headers needlessly
awkward. Neo therefore implements one narrow shape-expression evaluator. This
is not a general preprocessor and cannot provide field payloads.

Accepted declarations:

```c
#define CHANNEL_COUNT 4u
#define SAMPLE_COUNT (CHANNEL_COUNT * 8u)

typedef enum {
    AXIS_COUNT = 3
} dimensions_t;

uint16_t samples[CHANNEL_COUNT][SAMPLE_COUNT];
int16_t axes[AXIS_COUNT];
```

Accepted extent expressions contain:

- decimal, hexadecimal and octal integer literals;
- integer suffixes that do not change the represented value;
- same-file object-like macros available when the extent is used, and
  previously declared enum constants;
- parentheses;
- unary `+`; and
- `+`, `-`, `*`, `/` and `%`.

Evaluation uses checked unsigned 128-bit intermediates. Every intermediate
must be non-negative, and the final extent must be positive and fit `u64`.
Division by zero, subtraction below zero and overflow are errors. Integer
suffixes are accepted but do not change this shape-only arithmetic model.
Names form a dependency graph; cycles report every participating declaration.
Enumerators without initialisers follow C's implicit numbering rules. Shape
expansion is limited to 128 dependency levels and 16,384 visited tokens per
extent. Parenthesised expressions are limited to 128 nested levels.

Rejected expressions include function-like macros, token pasting,
stringification, casts, `sizeof`, `_Alignof`, `offsetof`, the ternary operator
and compiler built-ins. Unary minus, negative intermediate results, shifts,
bitwise operators and complement are excluded because their C results can
depend on integer widths and promotions that Neo does not otherwise model.

This evaluator exists only because symbolic array extents are common and shape
critical. A `#define` or enum constant never populates a field. Field payload
numbers still come only from JSON.

### Rejected reachable types

- Pointers and function pointers.
- Unions.
- C bitfields.
- Enum-typed members.
- C11 anonymous members.
- Flexible array members and variable-length arrays.
- Packed or explicitly aligned records.
- Atomic types.
- Function, object or linker symbol dependencies.
- Types whose complete definition comes from another file.

## Semantic type graph

The semantic IR is separate from Mint v2's TOML-shaped `Entry` tree:

```text
type
  scalar: one supported scalar representation
  alias: reference to another declared type
  record: ordered list of fields
  array: element type and ordered list of dimensions

field
  name: C identifier
  type: reference to a resolved type
  source: location in the header
  fingerprint: whether Neo supplies the value
```

The graph contains named declarations, but resolved layout and fingerprints
operate on canonical structural nodes with aliases removed. Alias resolution
uses an iterative walk with at most 128 consecutive aliases.
Records use memoised depth-first traversal with explicit visiting states and
cached structural heights, so depth limits are independent of traversal order.
A cycle through by-value members is an error with a diagnostic at every edge. Source spans and
the `fingerprint` marker are emission metadata and do not enter the structural
hash.

Resolution has explicit resource limits: 128 nested record levels, 16 array
dimensions and a 256 MiB resolved root. Element-count multiplication is
checked before allocation. These limits produce diagnostics rather than stack
overflow or process abort.

## ABI resolution

Mint Neo keeps the effective rules of Mint v2:

- byte order;
- addressable-unit width;
- scalar storage size and alignment;
- scalar array stride;
- aggregate alignment;
- aggregate tail padding; and
- C28x restrictions on exact-width 8-bit types.

Two ABI-family details remain explicit:

- `tricore-eabi-le` and `ti-c28x-eabi` align 64-bit scalars to 4 octets while
  retaining 8-octet storage.
- The same family raises the alignment of a record occupying more than one
  octet of unpadded member extent to at least 2 octets. Arrays do not receive
  a separate minimum; their alignment is their element's resolved alignment.

Fields are placed in declaration order. Each field begins at the next offset
valid for its alignment. Every nested record and the root record receive the
tail padding required by their alignment, because that padding is part of C
`sizeof`.

Arrays use the resolved, tail-padded `sizeof` of their element as their stride.
Neo also requires every scalar ABI's array stride to equal its storage size,
as C requires `sizeof(T[1]) == sizeof(T)`. An ABI profile that violates this
invariant is not valid for Neo.

The configured padding byte fills:

- alignment gaps between fields;
- nested record tail padding;
- array element tail padding; and
- root record tail padding required by its alignment.

Neo adds no bytes after the root record's resolved size in octets. For an ABI
whose C addressable unit is wider than one octet, this octet count equals the
C `sizeof` multiplied by the ABI's addressable-unit width in octets. There is
no allocated length and no padding to a reserved flash region.

The root's octet start address must satisfy the root record's resolved
alignment. A misaligned `@mint start-address` is an error.

## JSON contract

The input is one JSON object representing the root record. There is no variant
wrapper and no fallback:

```c
typedef struct {
    uint32_t id;
    point_t origin;
    point_t samples[2];
} config_t;
```

```json
{
  "id": 42,
  "origin": {
    "x": 10,
    "y": 20
  },
  "samples": [
    { "x": 1, "y": 2 },
    { "x": 3, "y": 4 }
  ]
}
```

Binding rules:

- Record fields map to object properties with the same spelling.
- Arrays map to JSON arrays at every dimension.
- Every array length must equal its declared C extent.
- Every ordinary leaf field is required.
- The one optional `@mint fingerprint` field must be absent.
- Extra object properties are errors.
- Duplicate object properties are errors; parsing never uses last-value-wins
  semantics. Escaped and literal spellings of the same key count as duplicates.
- JSON values may be nested at most 256 levels below the root.
- `null` is never a fallback and is invalid for every supported field.
- JSON booleans and strings are invalid for every first-version field type.
- Integer JSON values must be integral and fit the exact destination range.
- Fraction or exponent syntax is accepted for an integer only when its
  mathematical value is exactly integral and in range.
- Floating-point fields accept finite JSON numbers. Conversion to binary32 or
  binary64 uses IEEE round-to-nearest, ties-to-even. This defined decimal to
  binary conversion is not treated as a strictness failure; finite-range
  overflow is an error.
- Integer conversion never rounds, clamps, truncates or silently fills.
- Numeric tokens are preserved until their destination type is known. An
  integer too large for Neo's supported range is diagnosed from its original
  source text rather than first being rounded through binary floating point.
- JSON object order has no effect.

The first version has no special JSON string-to-C-array conversion. A byte
array is supplied as an exact JSON array of integers. String semantics,
termination and fill policy can be added only after they have one explicit C
contract.

Upstream tools own defaults, merges, variants, unit conversion, fixed-point
conversion, bitmap packing and application-level validation.

## Fingerprints

Fingerprinting remains a core feature even though Neo does not generate a
header.

Neo uses a separate domain:

```text
mint neo block ABI fingerprint v1
```

The fingerprint includes:

- effective byte order and addressable-unit width;
- structural record and array shape;
- field offsets, storage sizes and alignments;
- array dimensions and strides; and
- scalar representations.

The fingerprint excludes:

- the Neo, block, typedef and field names;
- source locations and comments;
- JSON values;
- the block start address; and
- the selected padding byte value.

The fingerprint field contributes to the shape as an ordinary `uint64_t`.
Its generated value is not recursively hashed.

`fingerprint` requires no JSON:

```bash
mint-neo fingerprint config.h
```

Stdout contains exactly 16 zero-padded lowercase hexadecimal digits followed
by one newline. Diagnostics use stderr.
CMake can inject it into firmware without rewriting the schema:

```cmake
set_property(
  DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS
  "${CMAKE_CURRENT_SOURCE_DIR}/config.h"
)

execute_process(
  COMMAND mint-neo fingerprint
          "${CMAKE_CURRENT_SOURCE_DIR}/config.h"
  OUTPUT_VARIABLE CONFIG_FINGERPRINT
  OUTPUT_STRIP_TRAILING_WHITESPACE
  COMMAND_ERROR_IS_FATAL ANY
)

target_compile_definitions(
  firmware PRIVATE
  "CONFIG_SCHEMA_FINGERPRINT=0x${CONFIG_FINGERPRINT}ULL"
)
```

When the root contains an `@mint fingerprint` field, `build` writes the same
value into that field. Firmware can compare it with the build definition.

## Output

`build` emits one standard Intel HEX file:

```bash
mint-neo build config.h --json config.json --out config.hex
```

The output:

- starts at the octet address derived from `@mint start-address`;
- contains exactly the root record's resolved size in octets;
- uses standard octet-addressed Intel HEX addresses;
- uses the I32HEX form and 32-octet data records, except for a shorter final
  record;
- emits an extended linear address record before the first data record,
  including for addresses below 64 KiB, and whenever the upper address changes;
- uses LF line endings and ends with a newline after the end-of-file record;
  and
- fails if the octet range exceeds the supported 32-bit output address space.

For an ABI with addressable units wider than one octet, Neo applies the same
target-address to octet-address conversion as Mint v2.

Range combination, gap fill, reserved-region padding, checksums, signatures and
format conversion belong downstream.

## Command-line interface

The initial CLI surface is:

```text
mint-neo build HEADER --json FILE|- --out FILE
mint-neo fingerprint HEADER
mint-neo inspect HEADER [--format text|json]
mint-neo abi list
mint-neo abi show ABI
```

There are no block selectors because one file has exactly one root. There are
no format, variant, strictness, record-width, stats or post-processing options.

`inspect` reports:

- the selected ABI and output start address;
- root size in octets and target addressable units, plus alignment;
- every reachable field's path, type, offset, size and alignment;
- every array's dimensions and stride;
- alignment-padding ranges;
- the total number of alignment-padding octets; and
- the ABI fingerprint.

Human-readable output is the default. `inspect --format json` provides a
stable machine-readable description for build tooling and tests.

Successful commands return exit code 0. Schema, data and encoding failures
return 1. CLI usage failures return 2. Diagnostics use source excerpts on
stderr; machine-readable command output never shares stdout with diagnostics.

## Diagnostics

CLI diagnostics carry:

- a stable category;
- the input filename or logical source name;
- a byte span and line/column;
- the primary message; and
- related spans for duplicate declarations, unresolved types and cycles.

Header and JSON input always arrive as source text in the first version, so
both can retain exact spans. A data error also includes an RFC 6901 JSON pointer
such as `/samples/1/x`.

## Parser strategy

Use `tree-sitter-c` as a concrete syntax parser. Do not invoke Doxygen and do
not add ast-grep as an intermediate query layer.

- Tree-sitter handles C declaration and declarator syntax and supplies source
  spans.
- A small comment parser extracts strict `@mint` tags from Doxygen comments.
- A semantic visitor accepts the closed schema dialect and rejects unsupported
  reachable constructs.
- Symbol tables resolve tags, typedefs and shape constants. Macro bodies are
  token blobs in the C syntax tree, so shape expressions use a dedicated small
  lexer and expression parser.
- A separate type graph performs dependency and layout resolution.
- `serde_json` parses JSON syntax. A small adapter uses borrowed `RawValue`
  slices to retain number tokens and byte spans. An ordered map visitor keeps
  all keys so duplicate decoded names are rejected before binding.

Tree-sitter error recovery is not acceptance. Any `ERROR` or `MISSING` node
anywhere in the file is a fatal diagnostic. Unsupported attributes and
directives require active checks because they can be valid C syntax without
producing parser errors.

## Crate structure

Start in the existing workspace so ABI behaviour and comparative tests stay
close:

```text
crates/mint-neo/
  src/
    lib.rs
    main.rs
    syntax/
    annotation/
    constants/
    types/
    layout/
    json/
    fingerprint/
    output/
```

One package provides the `mint-neo` binary. An internal library target can keep
the implementation testable, but the first version makes no stable public API
commitment. Splitting core and CLI crates before such an API exists adds little
value.

Mint Neo must not depend on Mint v2's TOML `Config`, `Entry`, `DataSource` or
header generator. Those types cannot represent arrays of records and carry
policies Neo intentionally removes.

Neo currently owns its ABI and scalar definitions, with comparative tests
against `mint-core`. A shared ABI crate is deferred to a separate PR after
the decision to retain Neo. The likely shared boundary is:

- named ABI profiles and scalar representations;
- scalar-to-byte conversion with Neo's integer and floating-point policies;
- common address-range validation.

Fingerprint domains and semantic layout trees remain tool-specific.
Neo also owns its fixed-policy Intel HEX writer; Mint v2's renderer has
different record-selection and newline policies.
The shared crate returns data-oriented errors; Mint v2 and Neo convert them
into their own public diagnostic types.

## Validation strategy

The implementation should be validated at four boundaries.

### Parser and diagnostics

- Acceptance fixtures for every supported declaration spelling.
- Rejection fixtures for every excluded reachable construct.
- Exact span checks for malformed tags, unresolved names and cycles.
- Fuzzing or property tests that ensure malformed input never panics.

### ABI layout

- Paired Neo header and Mint v2 TOML fixtures for their common subset.
- Equal offsets, alignments, root sizes and emitted bytes for those fixtures.
- Existing target-compiler probes extended with Neo-authored headers.
- Dedicated arrays-of-records probes for nested dimensions and tail stride.
- The licensed TriCore probe extended with nested records and arrays of records;
  these shapes cannot rely only on the open Nix compiler matrix.

### JSON binding

- Exact nested records and arrays.
- Missing, extra, wrong-type, `null`, overflow and dimension mismatch errors.
- Property tests over generated values at every scalar boundary.

### Fingerprint and output

- Pinned Neo v1 fingerprint fixtures.
- Proof that names, comments, JSON, start address and padding byte do not
  change the fingerprint.
- Proof that structural, scalar, dimensional and ABI changes do.
- Proof that typedef'd and directly declared forms of the same array shape hash
  equally.
- Proof that layout-equivalent dimensions such as `[2][6]` and `[12]` hash
  differently.
- Golden Intel HEX output for 8-bit and wider address-unit ABIs.

## Decisions deliberately deferred

- The final product and crate name.
- Whether a later release should expose a stable library API or language
  bindings.
- String and character-array semantics.
- Native C integer and character spellings: `char`, `short`, `int`, `long`,
  `long long`, `bool`, `size_t` and enum-typed members.
- A stable machine-readable `inspect` schema version.
- Whether a later release may support more than one independent root.

These decisions do not block proof of the central model: one self-contained
C-shaped schema, one exact JSON object and one deterministic encoded range.
