use std::collections::{HashMap, HashSet};

use tree_sitter::Node;

use crate::abi::{Abi, Scalar};
use crate::annotation::{MintTags, parse_comment};
use crate::constants::{EnumConstant, EnumValue, ShapeEnv, evaluate};
use crate::diagnostic::Error;
use crate::source::Span;
use crate::syntax::{ParsedFile, collect_macros, descendants, file_scope_nodes};

pub const MAX_RECORD_DEPTH: usize = 128;
pub const MAX_TYPEDEF_DEPTH: usize = 128;
pub const MAX_ARRAY_DIMENSIONS: usize = 16;
pub const MAX_RESOLVED_SIZE: usize = 256 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct TypeId(pub usize);

#[derive(Clone, Debug)]
pub enum TypeKind {
    Scalar {
        scalar: Scalar,
    },
    Record {
        fields: Vec<Field>,
    },
    Array {
        element: TypeId,
        dimensions: Vec<u64>,
    },
}

#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub type_id: TypeId,
    pub span: Span,
    pub fingerprint: bool,
    pub spelling: String,
}

#[derive(Clone, Debug)]
pub struct SchemaTypes {
    pub abi: Abi,
    pub start_address: u32,
    pub start_address_span: Span,
    pub padding: u8,
    pub root_span: Span,
    pub root: TypeId,
    pub types: Vec<TypeKind>,
}

pub fn compile_types(parsed: &ParsedFile<'_>) -> Result<SchemaTypes, Error> {
    let macros = collect_macros(parsed)?;
    let attachments = collect_attachments(parsed)?;
    let mut env = ShapeEnv::new();
    for macro_def in macros {
        env.insert_macro(macro_def);
    }
    collect_enum_constants(parsed, &mut env)?;

    let root = find_root(parsed, &attachments)?;
    let abi_text = root.tags.abi.as_ref().ok_or_else(|| {
        schema(
            parsed,
            root.span,
            "@mint abi is required on the root record",
        )
    })?;
    let abi = crate::abi::parse_abi(&abi_text.0, parsed.source, abi_text.1)?;
    let (start_address, start_address_span) = root.tags.start_address.ok_or_else(|| {
        schema(
            parsed,
            root.span,
            "@mint start-address is required on the root record",
        )
    })?;
    let padding = root.tags.padding.map(|(value, _)| value).unwrap_or(0xFF);
    if root.tags.fingerprint.is_some() {
        return Err(schema(
            parsed,
            root.span,
            "@mint fingerprint is only valid on a root member",
        ));
    }

    let mut resolver = Resolver {
        parsed,
        attachments,
        env,
        abi,
        types: Vec::new(),
        record_heights: Vec::new(),
        memo: HashMap::new(),
        typedefs: HashMap::new(),
        struct_defs: HashMap::new(),
        visiting: HashMap::new(),
    };
    resolver.walk_index(parsed.root())?;
    let root_id = resolver.resolve_root(root.node)?;
    ensure_fingerprint_annotations(&resolver, root_id)?;
    ensure_single_fingerprint(&resolver, root_id)?;

    Ok(SchemaTypes {
        abi,
        start_address,
        start_address_span,
        padding,
        root_span: root.span,
        root: root_id,
        types: resolver.types,
    })
}

struct RootDecl<'tree> {
    node: Node<'tree>,
    span: Span,
    tags: MintTags,
}

fn collect_attachments(parsed: &ParsedFile<'_>) -> Result<HashMap<usize, MintTags>, Error> {
    let mut attachments = HashMap::new();
    for comment in descendants(parsed.root(), true)
        .into_iter()
        .filter(|node| node.kind() == "comment")
    {
        let Some(tags) = parse_comment(
            parsed.source,
            ParsedFile::span(comment),
            parsed.text(comment),
        )?
        else {
            continue;
        };
        let target = comment.next_named_sibling().filter(|node| {
            matches!(node.kind(), "type_definition" | "declaration" | "field_declaration")
                && parsed.source.only_whitespace(comment.end_byte(), node.start_byte())
                && !parsed.source.has_blank_line(comment.end_byte(), node.start_byte())
        }).ok_or_else(|| schema(parsed, tags.span, "@mint comment does not attach to a declaration; place one /** ... */ comment immediately before it"))?;
        if tags.has_block_metadata() && target.kind() != "type_definition" {
            return Err(schema(
                parsed,
                tags.span,
                "block metadata may appear only on the root record",
            ));
        }
        if tags.fingerprint.is_some() && target.kind() != "field_declaration" {
            return Err(schema(
                parsed,
                tags.span,
                "@mint fingerprint is only valid on a root member",
            ));
        }
        attachments.insert(target.start_byte(), tags);
    }
    Ok(attachments)
}

fn find_root<'tree>(
    parsed: &'tree ParsedFile<'tree>,
    attachments: &HashMap<usize, MintTags>,
) -> Result<RootDecl<'tree>, Error> {
    let mut found = None;
    let mut typedefs: Vec<_> = descendants(parsed.root(), true)
        .into_iter()
        .filter(|node| node.kind() == "type_definition")
        .collect();
    typedefs.sort_by_key(Node::start_byte);
    for child in typedefs {
        let Some(tags) = attachments.get(&child.start_byte()) else {
            continue;
        };
        if tags.block.is_none() {
            if tags.has_block_metadata() {
                return Err(schema(
                    parsed,
                    tags.span,
                    "block metadata may appear only on the root record",
                ));
            }
            continue;
        }
        if found.is_some() {
            return Err(schema(
                parsed,
                ParsedFile::span(child),
                "exactly one @mint block typedef is allowed",
            ));
        }
        if child
            .parent()
            .is_none_or(|parent| parent.kind() != "translation_unit")
        {
            return Err(schema(
                parsed,
                ParsedFile::span(child),
                "the root typedef must be at file scope",
            ));
        }
        let declarators = field_nodes(child, "declarator");
        if declarators.len() != 1 {
            return Err(schema(
                parsed,
                ParsedFile::span(child),
                "an annotated typedef must introduce exactly one name",
            ));
        }
        declarator_name(parsed, declarators[0])?;
        found = Some(RootDecl {
            node: child,
            span: ParsedFile::span(child),
            tags: tags.clone(),
        });
    }
    match found {
        Some(root) => Ok(root),
        None => Err(schema(
            parsed,
            Span::point(0),
            "header must contain exactly one @mint block typedef",
        )),
    }
}

struct Resolver<'a> {
    parsed: &'a ParsedFile<'a>,
    attachments: HashMap<usize, MintTags>,
    env: ShapeEnv,
    abi: Abi,
    types: Vec<TypeKind>,
    record_heights: Vec<usize>,
    memo: HashMap<usize, TypeId>,
    typedefs: HashMap<String, TypedefDef<'a>>,
    struct_defs: HashMap<String, Node<'a>>,
    visiting: HashMap<usize, Span>,
}

#[derive(Clone, Copy)]
struct TypedefDef<'a> {
    node: Node<'a>,
    declarator: Node<'a>,
}

impl<'a> Resolver<'a> {
    fn walk_index(&mut self, node: Node<'a>) -> Result<(), Error> {
        for node in file_scope_nodes(node) {
            match node.kind() {
                "type_definition" => self.index_typedef(node)?,
                "struct_specifier" => self.register_struct_tag(node)?,
                _ => {}
            }
        }
        Ok(())
    }

    fn index_typedef(&mut self, node: Node<'a>) -> Result<(), Error> {
        for declarator in field_nodes(node, "declarator") {
            if let Ok(name) = declarator_name(self.parsed, declarator)
                && let Some(prev) = self
                    .typedefs
                    .insert(name.clone(), TypedefDef { node, declarator })
            {
                return Err(schema(
                    self.parsed,
                    ParsedFile::span(node),
                    format!("duplicate typedef '{name}'"),
                )
                .related(ParsedFile::span(prev.node), "previous definition"));
            }
        }
        Ok(())
    }

    fn register_struct_tag(&mut self, spec: Node<'a>) -> Result<(), Error> {
        if spec.child_by_field_name("body").is_none() {
            return Ok(());
        }
        let Some(name) = spec.child_by_field_name("name") else {
            return Ok(());
        };
        let tag = self.parsed.text(name).to_owned();
        if let Some(prev) = self.struct_defs.insert(tag.clone(), spec) {
            return Err(schema(
                self.parsed,
                ParsedFile::span(spec),
                format!("duplicate struct tag '{tag}'"),
            )
            .related(ParsedFile::span(prev), "previous definition"));
        }
        Ok(())
    }

    fn resolve_root(&mut self, node: Node<'a>) -> Result<TypeId, Error> {
        self.reject_unsupported_on(node)?;
        let spec = node.child_by_field_name("type").ok_or_else(|| {
            schema(
                self.parsed,
                ParsedFile::span(node),
                "root typedef is missing a type",
            )
        })?;
        let declarators = field_nodes(node, "declarator");
        let type_id = self.resolve_spec(spec, 0, spec.start_byte())?;
        let type_id = self.apply_declarator(type_id, declarators[0])?;
        match &self.types[type_id.0] {
            TypeKind::Record { fields } if !fields.is_empty() => {}
            TypeKind::Record { .. } => {
                return Err(schema(
                    self.parsed,
                    ParsedFile::span(node),
                    "the root record must have at least one named member",
                ));
            }
            _ => {
                return Err(schema(
                    self.parsed,
                    ParsedFile::span(node),
                    "the @mint block typedef must name a complete record type",
                ));
            }
        }
        Ok(type_id)
    }

    fn resolve_spec(
        &mut self,
        mut spec: Node<'a>,
        depth: usize,
        complete_at: usize,
    ) -> Result<TypeId, Error> {
        // Follow aliases iteratively. Record recursion does not consume the
        // alias-chain budget, and cached records retain their structural height.
        let mut aliases = Vec::new();
        let mut type_id = loop {
            self.reject_unsupported_on(spec)?;
            if let Some(id) = self.memo.get(&spec.start_byte()).copied() {
                break self.check_record_depth(id, depth, ParsedFile::span(spec))?;
            }
            match spec.kind() {
                "primitive_type" | "type_identifier" => {
                    let name = self.parsed.text(spec);
                    let builtin = resolve_builtin(name)
                        .map_err(|message| schema(self.parsed, ParsedFile::span(spec), message))?;
                    let def = self.typedefs.get(name).copied();
                    if let Some(scalar) = builtin
                        && def.is_none_or(|def| def.node.end_byte() > spec.start_byte())
                    {
                        self.abi.scalar(scalar).map_err(|message| {
                            schema(self.parsed, ParsedFile::span(spec), message)
                        })?;
                        break self.push(TypeKind::Scalar { scalar });
                    }
                    let def = def.ok_or_else(|| {
                        schema(
                            self.parsed,
                            ParsedFile::span(spec),
                            format!("unknown type '{name}'"),
                        )
                    })?;
                    if def.node.end_byte() > spec.start_byte() {
                        return Err(schema(
                            self.parsed,
                            ParsedFile::span(spec),
                            format!("type '{name}' is not declared before this use"),
                        )
                        .related(ParsedFile::span(def.node), "declaration appears later"));
                    }
                    if aliases.len() >= MAX_TYPEDEF_DEPTH {
                        return Err(schema(
                            self.parsed,
                            ParsedFile::span(def.node),
                            "typedef alias chain exceeds 128 levels",
                        ));
                    }
                    self.reject_unsupported_on(def.node)?;
                    aliases.push((def, builtin));
                    spec = def.node.child_by_field_name("type").ok_or_else(|| {
                        schema(
                            self.parsed,
                            ParsedFile::span(def.node),
                            "typedef is missing a type",
                        )
                    })?;
                }
                "struct_specifier" => break self.resolve_struct(spec, depth, complete_at)?,
                "enum_specifier" => {
                    return Err(schema(
                        self.parsed,
                        ParsedFile::span(spec),
                        "enum-typed members are not supported",
                    ));
                }
                "union_specifier" => {
                    return Err(schema(
                        self.parsed,
                        ParsedFile::span(spec),
                        "unions are not supported in reachable types",
                    ));
                }
                _ => {
                    return Err(schema(
                        self.parsed,
                        ParsedFile::span(spec),
                        format!("unsupported type spelling '{}'", self.parsed.text(spec)),
                    ));
                }
            }
        };
        for (def, builtin) in aliases.into_iter().rev() {
            type_id = self.apply_declarator(type_id, def.declarator)?;
            if let Some(expected) = builtin
                && !matches!(self.types[type_id.0], TypeKind::Scalar { scalar } if scalar == expected)
            {
                return Err(schema(
                    self.parsed,
                    ParsedFile::span(def.node),
                    "typedef conflicts with its built-in scalar representation",
                ));
            }
        }
        Ok(type_id)
    }

    fn resolve_struct(
        &mut self,
        spec: Node<'a>,
        depth: usize,
        complete_at: usize,
    ) -> Result<TypeId, Error> {
        if depth >= MAX_RECORD_DEPTH {
            return Err(schema(
                self.parsed,
                ParsedFile::span(spec),
                format!("record nesting exceeds {MAX_RECORD_DEPTH} levels"),
            ));
        }
        if let Some(id) = self.memo.get(&spec.start_byte()).copied() {
            return self.check_record_depth(id, depth, ParsedFile::span(spec));
        }
        let body = if let Some(body) = spec.child_by_field_name("body") {
            self.reject_unsupported_on(spec)?;
            body
        } else if let Some(name) = spec.child_by_field_name("name") {
            let tag = self.parsed.text(name);
            let def = *self.struct_defs.get(tag).ok_or_else(|| {
                schema(
                    self.parsed,
                    ParsedFile::span(spec),
                    format!("incomplete struct '{tag}'"),
                )
            })?;
            if def.end_byte() > complete_at {
                return Err(schema(
                    self.parsed,
                    ParsedFile::span(spec),
                    format!("struct '{tag}' is incomplete at this use"),
                )
                .related(ParsedFile::span(def), "complete definition appears later"));
            }
            return self.resolve_struct(def, depth, complete_at);
        } else {
            return Err(schema(
                self.parsed,
                ParsedFile::span(spec),
                "incomplete unnamed struct",
            ));
        };
        if self
            .visiting
            .insert(spec.start_byte(), ParsedFile::span(spec))
            .is_some()
        {
            return self.cycle_error();
        }
        let mut fields = Vec::new();
        let mut names = HashMap::new();
        let mut cursor = body.walk();
        for child in body.named_children(&mut cursor) {
            if child.kind() != "field_declaration" {
                continue;
            }
            let field = self.resolve_field(child, depth + 1)?;
            if let Some(previous) = names.insert(field.name.clone(), field.span) {
                return Err(schema(
                    self.parsed,
                    field.span,
                    format!("duplicate member '{}'", field.name),
                )
                .related(previous, "previous member is here"));
            }
            fields.push(field);
        }
        if fields.is_empty() {
            self.visiting.remove(&spec.start_byte());
            return Err(schema(
                self.parsed,
                ParsedFile::span(spec),
                "every reachable record must have at least one named member",
            ));
        }
        let id = self.push(TypeKind::Record { fields });
        self.visiting.remove(&spec.start_byte());
        self.memo.insert(spec.start_byte(), id);
        Ok(id)
    }

    fn resolve_field(&mut self, node: Node<'a>, depth: usize) -> Result<Field, Error> {
        self.reject_unsupported_on(node)?;
        if has_named_child(node, "bitfield_clause") {
            return Err(schema(
                self.parsed,
                ParsedFile::span(node),
                "bitfields are not supported",
            ));
        }
        let spec = node.child_by_field_name("type").ok_or_else(|| {
            schema(
                self.parsed,
                ParsedFile::span(node),
                "field is missing a type",
            )
        })?;
        let declarators = field_nodes(node, "declarator");
        if declarators.is_empty() {
            return Err(schema(
                self.parsed,
                ParsedFile::span(node),
                "C11 anonymous members are not supported",
            ));
        }
        if declarators.len() != 1 {
            return Err(schema(
                self.parsed,
                ParsedFile::span(node),
                "reachable member declarations must introduce exactly one member",
            ));
        }
        let tags = self.attachments.get(&node.start_byte()).cloned();
        let type_id = self.resolve_spec(spec, depth, spec.start_byte())?;
        let type_id = self.apply_declarator(type_id, declarators[0])?;
        let name = declarator_name(self.parsed, declarators[0])?;
        let fingerprint = tags.as_ref().and_then(|tags| tags.fingerprint).is_some();
        if fingerprint && depth != 1 {
            return Err(schema(
                self.parsed,
                ParsedFile::span(node),
                "@mint fingerprint may appear only on a direct member of the root record",
            ));
        }
        if fingerprint {
            self.validate_fingerprint_field(type_id, node)?;
        }
        Ok(Field {
            name,
            type_id,
            span: ParsedFile::span(node),
            fingerprint,
            spelling: self.spelling(spec, declarators[0]),
        })
    }

    fn validate_fingerprint_field(&self, type_id: TypeId, node: Node<'_>) -> Result<(), Error> {
        match &self.types[type_id.0] {
            TypeKind::Scalar {
                scalar: Scalar::U64,
            } => Ok(()),
            TypeKind::Array { .. } => Err(schema(
                self.parsed,
                ParsedFile::span(node),
                "@mint fingerprint cannot be applied to an array",
            )),
            _ => Err(schema(
                self.parsed,
                ParsedFile::span(node),
                "@mint fingerprint must be a uint64_t field",
            )),
        }
    }

    fn apply_declarator(
        &mut self,
        mut type_id: TypeId,
        declarator: Node<'a>,
    ) -> Result<TypeId, Error> {
        let mut dims = Vec::new();
        self.walk_declarator(declarator, &mut dims)?;
        if dims.len() > MAX_ARRAY_DIMENSIONS {
            return Err(schema(
                self.parsed,
                ParsedFile::span(declarator),
                format!("arrays may have at most {MAX_ARRAY_DIMENSIONS} dimensions"),
            ));
        }
        if !dims.is_empty() {
            type_id = self.canonicalize_array(type_id, dims, ParsedFile::span(declarator))?;
        }
        Ok(type_id)
    }

    fn canonicalize_array(
        &mut self,
        mut element: TypeId,
        mut dimensions: Vec<u64>,
        span: Span,
    ) -> Result<TypeId, Error> {
        if let TypeKind::Array {
            element: inner,
            dimensions: inner_dims,
        } = &self.types[element.0]
        {
            let inner = *inner;
            dimensions.extend(inner_dims.iter().copied());
            element = inner;
        }
        if dimensions.len() > MAX_ARRAY_DIMENSIONS {
            return Err(schema(
                self.parsed,
                span,
                format!("arrays may have at most {MAX_ARRAY_DIMENSIONS} dimensions"),
            ));
        }
        Ok(self.push(TypeKind::Array {
            element,
            dimensions,
        }))
    }

    fn walk_declarator(&self, node: Node<'a>, dims: &mut Vec<u64>) -> Result<(), Error> {
        self.env
            .reject_macro_use(self.parsed.source, ParsedFile::span(node))?;
        match node.kind() {
            "identifier" | "field_identifier" | "type_identifier" => Ok(()),
            "primitive_type" if self.parsed.text(node).ends_with("_t") => Ok(()),
            "array_declarator" => {
                let inner = node.child_by_field_name("declarator").ok_or_else(|| {
                    schema(
                        self.parsed,
                        ParsedFile::span(node),
                        "array declarator is missing a name",
                    )
                })?;
                let size = node.child_by_field_name("size").ok_or_else(|| {
                    schema(
                        self.parsed,
                        ParsedFile::span(node),
                        "flexible and variable-length arrays are not supported",
                    )
                })?;
                if self.parsed.text(size).trim() == "*" {
                    return Err(schema(
                        self.parsed,
                        ParsedFile::span(size),
                        "variable-length arrays are not supported",
                    ));
                }
                self.walk_declarator(inner, dims)?;
                dims.push(evaluate(
                    self.parsed.source,
                    ParsedFile::span(size),
                    self.parsed.text(size),
                    &self.env,
                    self.abi,
                )?);
                Ok(())
            }
            "pointer_declarator" => Err(schema(
                self.parsed,
                ParsedFile::span(node),
                "pointers are not supported in reachable types",
            )),
            "function_declarator" => Err(schema(
                self.parsed,
                ParsedFile::span(node),
                "function types are not supported in reachable types",
            )),
            "parenthesized_declarator" => match first_named(node) {
                Some(child) => self.walk_declarator(child, dims),
                None => Ok(()),
            },
            "attributed_declarator" => Err(schema(
                self.parsed,
                ParsedFile::span(node),
                "attributes are not supported",
            )),
            other => Err(schema(
                self.parsed,
                ParsedFile::span(node),
                format!("unsupported declarator '{other}'"),
            )),
        }
    }

    fn push(&mut self, kind: TypeKind) -> TypeId {
        let id = TypeId(self.types.len());
        let height = match &kind {
            TypeKind::Scalar { .. } => 0,
            TypeKind::Array { element, .. } => self.record_heights[element.0],
            TypeKind::Record { fields } => {
                1 + fields
                    .iter()
                    .map(|field| self.record_heights[field.type_id.0])
                    .max()
                    .unwrap_or(0)
            }
        };
        self.record_heights.push(height);
        self.types.push(kind);
        id
    }

    fn check_record_depth(&self, id: TypeId, depth: usize, span: Span) -> Result<TypeId, Error> {
        if depth + self.record_heights[id.0] > MAX_RECORD_DEPTH {
            return Err(schema(
                self.parsed,
                span,
                "record nesting exceeds 128 levels",
            ));
        }
        Ok(id)
    }

    fn spelling(&self, spec: Node<'_>, declarator: Node<'_>) -> String {
        let mut text = self.parsed.text(spec).trim().to_owned();
        if let Some(dims) = array_suffix(self.parsed, declarator) {
            text.push_str(&dims);
        }
        text
    }

    fn reject_unsupported_on(&self, node: Node<'_>) -> Result<(), Error> {
        self.env
            .reject_macro_use(self.parsed.source, ParsedFile::span(node))?;
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.env
                .reject_macro_use(self.parsed.source, ParsedFile::span(child))?;
            match child.kind() {
                "attribute_specifier"
                | "attribute_declaration"
                | "ms_declspec_modifier"
                | "alignas_qualifier"
                | "gnu_asm_expression" => {
                    return Err(schema(
                        self.parsed,
                        ParsedFile::span(child),
                        "attributes and explicit alignment are not supported",
                    ));
                }
                "type_qualifier" => {
                    let text = self.parsed.text(child).trim();
                    if matches!(text, "const" | "volatile") {
                        continue;
                    }
                    return Err(schema(
                        self.parsed,
                        ParsedFile::span(child),
                        format!("qualifier '{text}' is not supported"),
                    ));
                }
                "storage_class_specifier" => {
                    return Err(schema(
                        self.parsed,
                        ParsedFile::span(child),
                        "storage-class specifiers are not supported on reachable types",
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn cycle_error(&self) -> Result<TypeId, Error> {
        let mut spans = self.visiting.values().copied();
        let Some(first) = spans.next() else {
            return Err(Error::schema(
                self.parsed.source,
                Span::point(0),
                "cyclic by-value record dependency",
            ));
        };
        let mut error = Error::schema(
            self.parsed.source,
            first,
            "cyclic by-value record dependency",
        )
        .related(first, "participates in the cycle");
        for span in spans {
            error = error.related(span, "participates in the cycle");
        }
        Err(error)
    }
}

fn ensure_fingerprint_annotations(resolver: &Resolver<'_>, root: TypeId) -> Result<(), Error> {
    let TypeKind::Record { fields } = &resolver.types[root.0] else {
        return Ok(());
    };
    let root_fields: HashSet<usize> = fields.iter().map(|field| field.span.start).collect();
    for (target, tags) in &resolver.attachments {
        let Some(span) = tags.fingerprint else {
            continue;
        };
        if !root_fields.contains(target) {
            return Err(schema(
                resolver.parsed,
                span,
                "@mint fingerprint may appear only on a direct member of the root record",
            ));
        }
    }
    Ok(())
}

fn ensure_single_fingerprint(resolver: &Resolver<'_>, root: TypeId) -> Result<(), Error> {
    let TypeKind::Record { fields } = &resolver.types[root.0] else {
        return Ok(());
    };
    let mut marked = fields.iter().filter(|field| field.fingerprint);
    let Some(_) = marked.next() else {
        return Ok(());
    };
    if let Some(extra) = marked.next() {
        return Err(schema(
            resolver.parsed,
            extra.span,
            "at most one @mint fingerprint field is allowed",
        ));
    }
    Ok(())
}

fn array_suffix(parsed: &ParsedFile<'_>, mut node: Node<'_>) -> Option<String> {
    let mut dims = Vec::new();
    loop {
        match node.kind() {
            "array_declarator" => {
                if let Some(size) = node.child_by_field_name("size") {
                    dims.push(format!("[{}]", parsed.text(size).trim()));
                }
                if let Some(inner) = node.child_by_field_name("declarator") {
                    node = inner;
                    continue;
                }
            }
            "parenthesized_declarator" => {
                if let Some(inner) = first_named(node) {
                    node = inner;
                    continue;
                }
            }
            _ => break,
        }
        break;
    }
    if dims.is_empty() {
        None
    } else {
        dims.reverse();
        Some(dims.join(""))
    }
}

fn field_nodes<'tree>(node: Node<'tree>, field: &str) -> Vec<Node<'tree>> {
    let mut nodes = Vec::new();
    let mut cursor = node.walk();
    for child in node.children_by_field_name(field, &mut cursor) {
        nodes.push(child);
    }
    nodes
}

fn has_named_child(node: Node<'_>, kind: &str) -> bool {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| child.kind() == kind)
}

fn first_named(node: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).next()
}

fn declarator_name(parsed: &ParsedFile<'_>, node: Node<'_>) -> Result<String, Error> {
    let mut current = node;
    loop {
        match current.kind() {
            "identifier" | "field_identifier" | "type_identifier" => {
                return Ok(parsed.text(current).to_owned());
            }
            "primitive_type" if parsed.text(current).ends_with("_t") => {
                return Ok(parsed.text(current).to_owned());
            }
            "array_declarator"
            | "pointer_declarator"
            | "function_declarator"
            | "attributed_declarator" => {
                current = current.child_by_field_name("declarator").ok_or_else(|| {
                    schema(
                        parsed,
                        ParsedFile::span(current),
                        "declarator is missing a name",
                    )
                })?;
            }
            "parenthesized_declarator" => {
                current = first_named(current).ok_or_else(|| {
                    schema(
                        parsed,
                        ParsedFile::span(current),
                        "declarator is missing a name",
                    )
                })?;
            }
            other => {
                return Err(schema(
                    parsed,
                    ParsedFile::span(current),
                    format!("unsupported declarator '{other}'"),
                ));
            }
        }
    }
}

fn resolve_builtin(name: &str) -> Result<Option<Scalar>, String> {
    Ok(Some(match name {
        "uint8_t" => Scalar::U8,
        "uint16_t" => Scalar::U16,
        "uint32_t" => Scalar::U32,
        "uint64_t" => Scalar::U64,
        "int8_t" => Scalar::I8,
        "int16_t" => Scalar::I16,
        "int32_t" => Scalar::I32,
        "int64_t" => Scalar::I64,
        "float32_t" => Scalar::F32,
        "float64_t" => Scalar::F64,
        "float" => Scalar::F32,
        "double" => Scalar::F64,
        "_Bool" | "bool" | "char" | "short" | "int" | "long" | "size_t" => {
            return Err(format!("scalar type '{name}' is not supported"));
        }
        _ => return Ok(None),
    }))
}

fn collect_enum_constants(parsed: &ParsedFile<'_>, env: &mut ShapeEnv) -> Result<(), Error> {
    for node in file_scope_nodes(parsed.root()) {
        if node.kind() != "enum_specifier" {
            continue;
        }
        let Some(body) = node.child_by_field_name("body") else {
            continue;
        };
        let mut previous = None;
        let mut cursor = body.walk();
        for child in body
            .named_children(&mut cursor)
            .filter(|node| node.kind() == "enumerator")
        {
            let name = child.child_by_field_name("name").ok_or_else(|| {
                schema(
                    parsed,
                    ParsedFile::span(child),
                    "enumerator is missing a name",
                )
            })?;
            let value = match child.child_by_field_name("value") {
                Some(expr) => EnumValue::Expression(parsed.text(expr).to_owned()),
                None => match previous {
                    Some(name) => EnumValue::Successor(name),
                    None => EnumValue::Expression("0".to_owned()),
                },
            };
            let span = ParsedFile::span(name);
            let name = parsed.text(name).to_owned();
            env.insert_enum(EnumConstant {
                name: name.clone(),
                span,
                value,
            });
            previous = Some(name);
        }
    }
    Ok(())
}

fn schema(parsed: &ParsedFile<'_>, span: Span, message: impl Into<String>) -> Error {
    Error::schema(parsed.source, span, message)
}
