use std::collections::HashMap;

use tree_sitter::Node;

use crate::abi::{Abi, Scalar};
use crate::annotation::{
    CommentKind, MintTags, attach_leading, attach_trailing, group_comments, parse_comment,
};
use crate::constants::{ShapeEnv, evaluate, evaluate_any};
use crate::diagnostic::{Category, Diagnostic, Error};
use crate::source::Span;
use crate::syntax::{Comment, ParsedFile, collect_comments, collect_macros};

pub const MAX_RECORD_DEPTH: usize = 128;
pub const MAX_ARRAY_DIMENSIONS: usize = 16;
pub const MAX_RESOLVED_SIZE: usize = 256 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct TypeId(pub usize);

#[derive(Clone, Debug)]
pub enum TypeKind {
    Scalar {
        scalar: Scalar,
        spelling: String,
    },
    Record {
        name: Option<String>,
        fields: Vec<Field>,
    },
    Array {
        element: TypeId,
        dimensions: Vec<u64>,
    },
    Enum,
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
    pub padding: u8,
    pub root_name: String,
    pub root: TypeId,
    pub types: Vec<TypeKind>,
    pub fingerprint_field: Option<String>,
}

pub fn compile_types(parsed: &ParsedFile<'_>) -> Result<SchemaTypes, Error> {
    let attachments = collect_attachments(parsed)?;
    let mut env = ShapeEnv::new();
    for macro_def in collect_macros(parsed)? {
        env.insert_macro(
            macro_def.name,
            macro_def.span,
            macro_def.body,
            macro_def.function_like,
        );
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
    let abi = crate::abi::parse_abi(&abi_text.0, &parsed.source.name, abi_text.1)?;
    let start_address = root
        .tags
        .start_address
        .ok_or_else(|| {
            schema(
                parsed,
                root.span,
                "@mint start-address is required on the root record",
            )
        })?
        .0;
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
        memo: HashMap::new(),
        typedefs: HashMap::new(),
        struct_defs: HashMap::new(),
        visiting: HashMap::new(),
    };
    resolver.index_file_scope()?;
    let root_id = resolver.resolve_root(root.node)?;
    let fingerprint_field = fingerprint_member(&resolver, root_id)?;

    Ok(SchemaTypes {
        abi,
        start_address,
        padding,
        root_name: root.name,
        root: root_id,
        types: resolver.types,
        fingerprint_field,
    })
}

struct RootDecl<'tree> {
    node: Node<'tree>,
    name: String,
    span: Span,
    tags: MintTags,
}

fn collect_attachments(parsed: &ParsedFile<'_>) -> Result<HashMap<usize, MintTags>, Error> {
    let raw: Vec<(Span, &str)> = collect_comments(parsed)
        .into_iter()
        .map(|Comment { span, text }| (span, text))
        .collect();
    let grouped = group_comments(parsed.source, &raw);
    let mut mint_comments = Vec::new();
    for comment in &grouped {
        if let Some(tags) = parse_comment(parsed.source, comment)? {
            mint_comments.push(tags);
        }
    }

    let mut targets = Vec::new();
    collect_targets(parsed.root(), &mut targets);
    let mut attachments: HashMap<usize, MintTags> = HashMap::new();
    for tags in mint_comments {
        let kind = tags.kind;
        let attached = match kind {
            Some(CommentKind::Trailing) => targets.iter().find(|target| {
                target.kind == TargetKind::Field
                    && attach_trailing(parsed.source, target.semicolon, tags.span.start)
            }),
            Some(CommentKind::Leading) => targets
                .iter()
                .filter(|target| target.span.start >= tags.span.end)
                .min_by_key(|target| target.span.start)
                .filter(|target| attach_leading(parsed.source, tags.span.end, target.span.start)),
            None => None,
        };
        let Some(target) = attached else {
            return Err(schema(
                parsed,
                tags.span,
                "@mint comment does not attach to a declaration",
            ));
        };
        let entry = attachments.entry(target.span.start).or_default();
        merge_tags(parsed, entry, tags)?;
    }
    Ok(attachments)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TargetKind {
    Decl,
    Field,
}

struct Target {
    span: Span,
    semicolon: usize,
    kind: TargetKind,
}

fn collect_targets(node: Node<'_>, targets: &mut Vec<Target>) {
    match node.kind() {
        "type_definition" | "declaration" => {
            targets.push(Target {
                span: ParsedFile::span(node),
                semicolon: node.end_byte().saturating_sub(1),
                kind: TargetKind::Decl,
            });
        }
        "field_declaration" => {
            targets.push(Target {
                span: ParsedFile::span(node),
                semicolon: node.end_byte().saturating_sub(1),
                kind: TargetKind::Field,
            });
        }
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_targets(child, targets);
    }
}

fn merge_tags(parsed: &ParsedFile<'_>, dst: &mut MintTags, src: MintTags) -> Result<(), Error> {
    if src.block.is_some() {
        if dst.block.is_some() {
            return Err(schema(parsed, src.span, "duplicate @mint block tag"));
        }
        dst.block = src.block;
    }
    if src.abi.is_some() {
        if dst.abi.is_some() {
            return Err(schema(parsed, src.span, "duplicate @mint abi tag"));
        }
        dst.abi = src.abi;
    }
    if src.start_address.is_some() {
        if dst.start_address.is_some() {
            return Err(schema(
                parsed,
                src.span,
                "duplicate @mint start-address tag",
            ));
        }
        dst.start_address = src.start_address;
    }
    if src.padding.is_some() {
        if dst.padding.is_some() {
            return Err(schema(parsed, src.span, "duplicate @mint padding tag"));
        }
        dst.padding = src.padding;
    }
    if src.fingerprint.is_some() {
        if dst.fingerprint.is_some() {
            return Err(schema(parsed, src.span, "duplicate @mint fingerprint tag"));
        }
        dst.fingerprint = src.fingerprint;
    }
    dst.span = if dst.span.is_empty() {
        src.span
    } else {
        dst.span.merge(src.span)
    };
    Ok(())
}

fn find_root<'tree>(
    parsed: &'tree ParsedFile<'tree>,
    attachments: &HashMap<usize, MintTags>,
) -> Result<RootDecl<'tree>, Error> {
    let mut found = None;
    let mut cursor = parsed.root().walk();
    for child in parsed.root().named_children(&mut cursor) {
        if child.kind() != "type_definition" {
            continue;
        }
        let Some(tags) = attachments.get(&child.start_byte()) else {
            continue;
        };
        if tags.block.is_none() {
            if tags.abi.is_some() || tags.start_address.is_some() || tags.padding.is_some() {
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
        let declarators = field_nodes(child, "declarator");
        if declarators.len() != 1 {
            return Err(schema(
                parsed,
                ParsedFile::span(child),
                "an annotated typedef must introduce exactly one name",
            ));
        }
        let name = declarator_name(parsed, declarators[0])?;
        found = Some(RootDecl {
            node: child,
            name,
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
    memo: HashMap<usize, TypeId>,
    typedefs: HashMap<String, Node<'a>>,
    struct_defs: HashMap<String, Node<'a>>,
    visiting: HashMap<usize, Span>,
}

impl<'a> Resolver<'a> {
    fn index_file_scope(&mut self) -> Result<(), Error> {
        let mut cursor = self.parsed.root().walk();
        for child in self.parsed.root().named_children(&mut cursor) {
            match child.kind() {
                "type_definition" => self.index_typedef(child)?,
                "declaration" | "struct_specifier" => self.index_struct_spec(child)?,
                _ => {}
            }
        }
        Ok(())
    }

    fn index_typedef(&mut self, node: Node<'a>) -> Result<(), Error> {
        if let Some(spec) = node.child_by_field_name("type") {
            self.index_struct_spec(spec)?;
        }
        for declarator in field_nodes(node, "declarator") {
            if let Ok(name) = declarator_name(self.parsed, declarator)
                && let Some(prev) = self.typedefs.insert(name.clone(), node)
            {
                return Err(Error::one(
                    schema_diag(
                        self.parsed,
                        ParsedFile::span(node),
                        format!("duplicate typedef '{name}'"),
                    )
                    .related(
                        &self.parsed.source.name,
                        ParsedFile::span(prev),
                        "previous definition",
                    ),
                ));
            }
        }
        Ok(())
    }

    fn index_struct_spec(&mut self, node: Node<'a>) -> Result<(), Error> {
        let spec = if node.kind() == "struct_specifier" {
            node
        } else if let Some(spec) = node.child_by_field_name("type") {
            spec
        } else {
            return Ok(());
        };
        if spec.kind() != "struct_specifier" {
            return Ok(());
        }
        if spec.child_by_field_name("body").is_none() {
            return Ok(());
        }
        if let Some(name) = spec.child_by_field_name("name") {
            let tag = self.parsed.text(name).to_owned();
            if let Some(prev) = self.struct_defs.insert(tag.clone(), spec) {
                return Err(Error::one(
                    schema_diag(
                        self.parsed,
                        ParsedFile::span(spec),
                        format!("duplicate struct tag '{tag}'"),
                    )
                    .related(
                        &self.parsed.source.name,
                        ParsedFile::span(prev),
                        "previous definition",
                    ),
                ));
            }
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
        let type_id = self.resolve_spec(spec, 0)?;
        let type_id = self.apply_declarator(type_id, declarators[0], 0)?;
        match &self.types[type_id.0] {
            TypeKind::Record { fields, .. } if !fields.is_empty() => {}
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

    fn resolve_spec(&mut self, spec: Node<'a>, depth: usize) -> Result<TypeId, Error> {
        self.reject_unsupported_on(spec)?;
        if let Some(id) = self.memo.get(&spec.start_byte()).copied() {
            return Ok(id);
        }
        match spec.kind() {
            "primitive_type" | "type_identifier" => {
                let name = self.parsed.text(spec);
                if let Some(scalar) = resolve_builtin(name, self.abi)
                    .map_err(|message| schema(self.parsed, ParsedFile::span(spec), message))?
                {
                    self.abi
                        .scalar(scalar)
                        .map_err(|message| schema(self.parsed, ParsedFile::span(spec), message))?;
                    return Ok(self.push(TypeKind::Scalar {
                        scalar,
                        spelling: name.to_owned(),
                    }));
                }
                if let Some(typedef) = self.typedefs.get(name).copied() {
                    return self.resolve_typedef_node(typedef, depth);
                }
                if let Some(record) = self.struct_defs.get(name).copied() {
                    return self.resolve_struct(record, depth);
                }
                Err(schema(
                    self.parsed,
                    ParsedFile::span(spec),
                    format!("unknown type '{name}'"),
                ))
            }
            "struct_specifier" => self.resolve_struct(spec, depth),
            "enum_specifier" => Ok(self.push(TypeKind::Enum)),
            "union_specifier" => Err(schema(
                self.parsed,
                ParsedFile::span(spec),
                "unions are not supported in reachable types",
            )),
            "sized_type_specifier" | "macro_type_specifier" => Err(schema(
                self.parsed,
                ParsedFile::span(spec),
                format!("unsupported type spelling '{}'", self.parsed.text(spec)),
            )),
            other => Err(schema(
                self.parsed,
                ParsedFile::span(spec),
                format!("unsupported type specifier '{other}'"),
            )),
        }
    }

    fn resolve_typedef_node(&mut self, node: Node<'a>, depth: usize) -> Result<TypeId, Error> {
        if let Some(id) = self.memo.get(&node.start_byte()).copied() {
            return Ok(id);
        }
        if self
            .visiting
            .insert(node.start_byte(), ParsedFile::span(node))
            .is_some()
        {
            return self.cycle_error();
        }
        let spec = node.child_by_field_name("type").ok_or_else(|| {
            schema(
                self.parsed,
                ParsedFile::span(node),
                "typedef is missing a type",
            )
        })?;
        let declarators = field_nodes(node, "declarator");
        if declarators.len() != 1 {
            self.visiting.remove(&node.start_byte());
            return Err(schema(
                self.parsed,
                ParsedFile::span(node),
                "reachable typedefs must introduce exactly one name",
            ));
        }
        let type_id = self.resolve_spec(spec, depth)?;
        let type_id = self.apply_declarator(type_id, declarators[0], depth)?;
        self.visiting.remove(&node.start_byte());
        self.memo.insert(node.start_byte(), type_id);
        Ok(type_id)
    }

    fn resolve_struct(&mut self, spec: Node<'a>, depth: usize) -> Result<TypeId, Error> {
        if depth > MAX_RECORD_DEPTH {
            return Err(schema(
                self.parsed,
                ParsedFile::span(spec),
                format!("record nesting exceeds {MAX_RECORD_DEPTH} levels"),
            ));
        }
        if let Some(id) = self.memo.get(&spec.start_byte()).copied() {
            return Ok(id);
        }
        let body = if let Some(body) = spec.child_by_field_name("body") {
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
            return self.resolve_struct(def, depth);
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
        let name = spec
            .child_by_field_name("name")
            .map(|node| self.parsed.text(node).to_owned());
        let mut fields = Vec::new();
        let mut cursor = body.walk();
        for child in body.named_children(&mut cursor) {
            if child.kind() != "field_declaration" {
                continue;
            }
            fields.push(self.resolve_field(child, depth + 1)?);
        }
        if fields.is_empty() {
            self.visiting.remove(&spec.start_byte());
            return Err(schema(
                self.parsed,
                ParsedFile::span(spec),
                "every reachable record must have at least one named member",
            ));
        }
        let id = self.push(TypeKind::Record { name, fields });
        self.visiting.remove(&spec.start_byte());
        self.memo.insert(spec.start_byte(), id);
        Ok(id)
    }

    fn resolve_field(&mut self, node: Node<'a>, depth: usize) -> Result<Field, Error> {
        self.reject_unsupported_on(node)?;
        if node.child_by_field_name("bitfield_clause").is_some()
            || has_named_child(node, "bitfield_clause")
        {
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
        if let Some(tags) = &tags
            && (tags.block.is_some()
                || tags.abi.is_some()
                || tags.start_address.is_some()
                || tags.padding.is_some())
        {
            return Err(schema(
                self.parsed,
                tags.span,
                "@mint block metadata is only valid on the root record",
            ));
        }
        let type_id = self.resolve_spec(spec, depth)?;
        if matches!(self.types[type_id.0], TypeKind::Enum) {
            return Err(schema(
                self.parsed,
                ParsedFile::span(node),
                "enum-typed members are not supported",
            ));
        }
        let type_id = self.apply_declarator(type_id, declarators[0], depth)?;
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
                ..
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
        depth: usize,
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
            type_id = self.canonicalize_array(type_id, dims, depth)?;
        }
        Ok(type_id)
    }

    fn canonicalize_array(
        &mut self,
        element: TypeId,
        mut dimensions: Vec<u64>,
        depth: usize,
    ) -> Result<TypeId, Error> {
        let mut element = self.peel_alias_like(element);
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
                Span::point(0),
                format!("arrays may have at most {MAX_ARRAY_DIMENSIONS} dimensions"),
            ));
        }
        let _ = depth;
        Ok(self.push(TypeKind::Array {
            element,
            dimensions,
        }))
    }

    fn walk_declarator(&self, node: Node<'a>, dims: &mut Vec<u64>) -> Result<(), Error> {
        match node.kind() {
            "identifier" | "field_identifier" | "type_identifier" => Ok(()),
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
            "parenthesized_declarator" => {
                let mut cursor = node.walk();
                if let Some(child) = node.named_children(&mut cursor).next() {
                    return self.walk_declarator(child, dims);
                }
                Ok(())
            }
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

    fn peel_alias_like(&self, id: TypeId) -> TypeId {
        id
    }

    fn push(&mut self, kind: TypeKind) -> TypeId {
        let id = TypeId(self.types.len());
        self.types.push(kind);
        id
    }

    fn spelling(&self, spec: Node<'_>, declarator: Node<'_>) -> String {
        let mut text = self.parsed.text(spec).trim().to_owned();
        if let Some(dims) = array_suffix(self.parsed, declarator) {
            text.push_str(&dims);
        }
        text
    }

    fn reject_unsupported_on(&self, node: Node<'_>) -> Result<(), Error> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
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
        let mut diagnostic = Diagnostic::new(
            Category::Schema,
            &self.parsed.source.name,
            "cyclic by-value record dependency",
        );
        for span in self.visiting.values() {
            diagnostic =
                diagnostic.related(&self.parsed.source.name, *span, "participates in the cycle");
            if diagnostic.span.is_none() {
                diagnostic.span = Some(*span);
            }
        }
        Err(Error::one(diagnostic))
    }
}

fn fingerprint_member(resolver: &Resolver<'_>, root: TypeId) -> Result<Option<String>, Error> {
    let TypeKind::Record { fields, .. } = &resolver.types[root.0] else {
        return Ok(None);
    };
    let marked: Vec<&Field> = fields.iter().filter(|field| field.fingerprint).collect();
    if marked.len() > 1 {
        return Err(schema(
            resolver.parsed,
            marked[1].span,
            "at most one @mint fingerprint field is allowed",
        ));
    }
    Ok(marked.first().map(|field| field.name.clone()))
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
                let mut cursor = node.walk();
                if let Some(inner) = node.named_children(&mut cursor).next() {
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

fn declarator_name(parsed: &ParsedFile<'_>, node: Node<'_>) -> Result<String, Error> {
    let mut current = node;
    loop {
        match current.kind() {
            "identifier" | "field_identifier" | "type_identifier" => {
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
                let mut cursor = current.walk();
                current = current.named_children(&mut cursor).next().ok_or_else(|| {
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

fn resolve_builtin(name: &str, abi: Abi) -> Result<Option<Scalar>, String> {
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
        "float" => {
            if abi.guarantees_ieee_float() {
                Scalar::F32
            } else {
                return Err(
                    "C float is not an IEEE-754 binary32 type on this ABI; use float32_t".into(),
                );
            }
        }
        "double" => {
            if abi.guarantees_ieee_double() {
                Scalar::F64
            } else {
                return Err(
                    "C double is not an IEEE-754 binary64 type on this ABI; use float64_t".into(),
                );
            }
        }
        "_Bool" | "bool" | "char" | "short" | "int" | "long" | "size_t" => {
            return Err(format!("scalar type '{name}' is not supported"));
        }
        _ => return Ok(None),
    }))
}

fn collect_enum_constants(parsed: &ParsedFile<'_>, env: &mut ShapeEnv) -> Result<(), Error> {
    let mut stack = vec![parsed.root()];
    let mut enums = Vec::new();
    while let Some(node) = stack.pop() {
        if node.kind() == "enum_specifier"
            && let Some(body) = node.child_by_field_name("body")
        {
            enums.push(body);
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            stack.push(child);
        }
    }
    enums.sort_by_key(|node| node.start_byte());
    for body in enums {
        let mut next = 0u128;
        let mut cursor = body.walk();
        for child in body.named_children(&mut cursor) {
            if child.kind() != "enumerator" {
                continue;
            }
            let name = child.child_by_field_name("name").ok_or_else(|| {
                schema(
                    parsed,
                    ParsedFile::span(child),
                    "enumerator is missing a name",
                )
            })?;
            let value = if let Some(expr) = child.child_by_field_name("value") {
                evaluate_any(
                    parsed.source,
                    ParsedFile::span(expr),
                    parsed.text(expr),
                    env,
                )?
            } else {
                next
            };
            let stored = u64::try_from(value).map_err(|_| {
                schema(
                    parsed,
                    ParsedFile::span(child),
                    "enumerator value does not fit u64",
                )
            })?;
            env.insert_constant(parsed.text(name).to_owned(), stored, ParsedFile::span(name));
            next = value.saturating_add(1);
        }
    }
    Ok(())
}

fn schema(parsed: &ParsedFile<'_>, span: Span, message: impl Into<String>) -> Error {
    Error::one(schema_diag(parsed, span, message))
}

fn schema_diag(parsed: &ParsedFile<'_>, span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Category::Schema, &parsed.source.name, message).at(span)
}
