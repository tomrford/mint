use std::collections::HashMap;

use tree_sitter::Node;

use crate::abi::{Abi, Scalar};
use crate::annotation::{
    CommentKind, MintTags, attach_leading, attach_trailing, group_comments, parse_comment,
};
use crate::constants::{ShapeEnv, evaluate, evaluate_any};
use crate::diagnostic::Error;
use crate::source::Span;
use crate::syntax::{Comment, ParsedFile, collect_comments_and_macros, descendants};

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
    let (comments, macros) = collect_comments_and_macros(parsed)?;
    let attachments = collect_attachments(parsed, comments)?;
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
        memo: HashMap::new(),
        typedefs: HashMap::new(),
        struct_defs: HashMap::new(),
        visiting: HashMap::new(),
        typedef_depth: 0,
    };
    resolver.walk_index(parsed.root())?;
    let root_id = resolver.resolve_root(root.node)?;
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

fn collect_attachments(
    parsed: &ParsedFile<'_>,
    comments: Vec<Comment<'_>>,
) -> Result<HashMap<usize, MintTags>, Error> {
    let raw: Vec<(Span, &str)> = comments
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
        reject_invalid_location(parsed, target, &tags)?;
        let span = tags.span;
        let entry = attachments.entry(target.span.start).or_default();
        if let Err(tag) = entry.merge(tags) {
            return Err(schema(parsed, span, format!("duplicate @mint {tag} tag")));
        }
    }
    Ok(attachments)
}

fn reject_invalid_location(
    parsed: &ParsedFile<'_>,
    target: &Target,
    tags: &MintTags,
) -> Result<(), Error> {
    if tags.has_block_metadata() && target.kind != TargetKind::Typedef {
        return Err(schema(
            parsed,
            tags.span,
            "block metadata may appear only on the root record",
        ));
    }
    if tags.fingerprint.is_some() && target.kind != TargetKind::Field {
        return Err(schema(
            parsed,
            tags.span,
            "@mint fingerprint is only valid on a root member",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TargetKind {
    Typedef,
    Declaration,
    Field,
}

struct Target {
    span: Span,
    semicolon: usize,
    kind: TargetKind,
}

fn collect_targets(root: Node<'_>, targets: &mut Vec<Target>) {
    for node in descendants(root, true) {
        let kind = match node.kind() {
            "type_definition" => TargetKind::Typedef,
            "declaration" => TargetKind::Declaration,
            "field_declaration" => TargetKind::Field,
            _ => continue,
        };
        targets.push(Target {
            span: ParsedFile::span(node),
            semicolon: node.end_byte().saturating_sub(1),
            kind,
        });
    }
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
    memo: HashMap<usize, TypeId>,
    typedefs: HashMap<String, TypedefDef<'a>>,
    struct_defs: HashMap<String, Node<'a>>,
    visiting: HashMap<usize, Span>,
    typedef_depth: usize,
}

#[derive(Clone, Copy)]
struct TypedefDef<'a> {
    node: Node<'a>,
    declarator: Node<'a>,
}

impl<'a> Resolver<'a> {
    fn walk_index(&mut self, node: Node<'a>) -> Result<(), Error> {
        match node.kind() {
            "type_definition" => self.index_typedef(node)?,
            "struct_specifier" => self.register_struct_tag(node)?,
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.walk_index(child)?;
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
        let type_id = self.resolve_spec(spec, 0)?;
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
                    return Ok(self.push(TypeKind::Scalar { scalar }));
                }
                if let Some(typedef) = self.typedefs.get(name).copied() {
                    return self.resolve_typedef_def(typedef, depth);
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
            "enum_specifier" => Err(schema(
                self.parsed,
                ParsedFile::span(spec),
                "enum-typed members are not supported",
            )),
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

    fn resolve_typedef_def(&mut self, def: TypedefDef<'a>, depth: usize) -> Result<TypeId, Error> {
        let key = def.declarator.start_byte();
        if let Some(id) = self.memo.get(&key).copied() {
            return Ok(id);
        }
        self.reject_unsupported_on(def.node)?;
        if self.typedef_depth >= MAX_TYPEDEF_DEPTH {
            return Err(schema(
                self.parsed,
                ParsedFile::span(def.node),
                format!("typedef alias chain exceeds {MAX_TYPEDEF_DEPTH} levels"),
            ));
        }
        if self
            .visiting
            .insert(key, ParsedFile::span(def.node))
            .is_some()
        {
            return self.cycle_error();
        }
        let spec = def.node.child_by_field_name("type").ok_or_else(|| {
            schema(
                self.parsed,
                ParsedFile::span(def.node),
                "typedef is missing a type",
            )
        })?;
        self.typedef_depth += 1;
        let resolved = self
            .resolve_spec(spec, depth)
            .and_then(|type_id| self.apply_declarator(type_id, def.declarator));
        self.typedef_depth -= 1;
        self.visiting.remove(&key);
        let type_id = resolved?;
        self.memo.insert(key, type_id);
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
        let type_id = self.resolve_spec(spec, depth)?;
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
            if abi.guarantees_ieee() {
                Scalar::F32
            } else {
                return Err(
                    "C float is not an IEEE-754 binary32 type on this ABI; use float32_t".into(),
                );
            }
        }
        "double" => {
            if abi.guarantees_ieee() {
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
    let mut enums = Vec::new();
    for node in descendants(parsed.root(), true) {
        if node.kind() == "enum_specifier"
            && let Some(body) = node.child_by_field_name("body")
        {
            enums.push(body);
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
            let name_text = parsed.text(name);
            let span = ParsedFile::span(name);
            if let Some(previous) = env.insert_constant(name_text.to_owned(), stored, span) {
                return Err(
                    schema(parsed, span, format!("duplicate enumerator '{name_text}'"))
                        .related(previous, "previous enumerator is here"),
                );
            }
            next = value.saturating_add(1);
        }
    }
    Ok(())
}

fn schema(parsed: &ParsedFile<'_>, span: Span, message: impl Into<String>) -> Error {
    Error::schema(parsed.source, span, message)
}
