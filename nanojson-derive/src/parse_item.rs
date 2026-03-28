use proc_macro::{Delimiter, Group, Ident, Span, TokenStream, TokenTree};
use crate::helpers::compiler_error;

// ---- Public types ----

pub(crate) struct ParsedItem {
    pub name: String,
    #[allow(dead_code)]
    pub name_span: Span,
    /// Raw generic params (everything between `<` and `>`, exclusive), or empty.
    pub generics: Vec<TokenTree>,
    pub kind: ItemKind,
}

pub(crate) enum ItemKind {
    Struct(Vec<ParsedField>),
    Enum(Vec<ParsedVariant>),
}

pub(crate) struct ParsedField {
    pub name: String,
    #[allow(dead_code)]
    pub name_span: Span,
    /// Raw token trees for the field type.
    pub ty: Vec<TokenTree>,
    /// JSON key name (from `#[nanojson(rename = "...")]` or the field ident).
    pub json_name: String,
}

pub(crate) struct ParsedVariant {
    pub name: String,
    #[allow(dead_code)]
    pub name_span: Span,
    pub fields: VariantFields,
    pub json_name: String,
}

pub(crate) enum VariantFields {
    Unit,
    Named(Vec<ParsedField>),
}

// ---- Token iterator helpers ----

struct Tokens {
    inner: Vec<TokenTree>,
    pos: usize,
}

impl Tokens {
    fn new(ts: TokenStream) -> Self {
        Self { inner: ts.into_iter().collect(), pos: 0 }
    }

    fn peek(&self) -> Option<&TokenTree> {
        self.inner.get(self.pos)
    }

    fn next(&mut self) -> Option<TokenTree> {
        if self.pos < self.inner.len() {
            let t = self.inner[self.pos].clone();
            self.pos += 1;
            Some(t)
        } else {
            None
        }
    }

    fn next_ident(&mut self) -> Result<Ident, TokenStream> {
        match self.next() {
            Some(TokenTree::Ident(i)) => Ok(i),
            Some(other) => compiler_error!(other, "expected identifier, got `{other}`"),
            None => compiler_error!("unexpected end of input, expected identifier"),
        }
    }

    /// Skip an ident if it matches; return whether it was skipped.
    fn skip_ident(&mut self, name: &str) -> bool {
        if let Some(TokenTree::Ident(i)) = self.peek() {
            if i.to_string() == name {
                self.pos += 1;
                return true;
            }
        }
        false
    }

    fn skip_visibility(&mut self) {
        // pub / pub(crate) / pub(super) / pub(in path)
        if let Some(TokenTree::Ident(i)) = self.peek() {
            if i.to_string() == "pub" {
                self.pos += 1;
                // absorb optional (...)
                if let Some(TokenTree::Group(g)) = self.peek() {
                    if g.delimiter() == Delimiter::Parenthesis {
                        self.pos += 1;
                    }
                }
            }
        }
    }

    /// Collect everything up to (not including) a `,` or end, consuming any trailing `,`.
    fn collect_until_comma(&mut self) -> Vec<TokenTree> {
        let mut out = Vec::new();
        loop {
            match self.peek() {
                None => break,
                Some(TokenTree::Punct(p)) if p.as_char() == ',' => {
                    self.pos += 1;
                    break;
                }
                _ => out.push(self.next().unwrap()),
            }
        }
        out
    }

    /// Collect generic params between `<` and matching `>`.
    /// Assumes we've already seen that the next token is `<`.
    fn collect_generics(&mut self) -> Result<Vec<TokenTree>, TokenStream> {
        // The proc_macro tokenizer may give us `<` as a Punct or might have
        // already grouped things. For simplicity we handle the common cases:
        // - No generics: return empty immediately.
        // - `<T, U>`: collect token trees tracking depth.
        let is_lt = match self.peek() {
            Some(TokenTree::Punct(p)) if p.as_char() == '<' => true,
            _ => false,
        };
        if !is_lt {
            return Ok(Vec::new());
        }
        self.next(); // consume `<`
        let mut out = Vec::new();
        let mut depth = 1usize;
        loop {
            match self.next() {
                None => return compiler_error!("unexpected end of input inside generics"),
                Some(TokenTree::Punct(p)) if p.as_char() == '<' => {
                    depth += 1;
                    out.push(TokenTree::Punct(p));
                }
                Some(TokenTree::Punct(p)) if p.as_char() == '>' => {
                    depth -= 1;
                    if depth == 0 { break; }
                    out.push(TokenTree::Punct(p));
                }
                Some(t) => out.push(t),
            }
        }
        Ok(out)
    }
}

// ---- Attribute parsing ----

/// Returns the JSON rename if `#[nanojson(rename = "...")]` is present in the attribute list.
fn parse_attrs(attrs: &[TokenTree]) -> Result<Option<String>, TokenStream> {
    let mut i = 0;
    while i < attrs.len() {
        // Look for `# [ ... ]`
        if let TokenTree::Punct(p) = &attrs[i] {
            if p.as_char() == '#' {
                i += 1;
                if i >= attrs.len() { break; }
                if let TokenTree::Group(g) = &attrs[i] {
                    if g.delimiter() == Delimiter::Bracket {
                        if let Some(rename) = parse_nanojson_attr(g.stream())? {
                            return Ok(Some(rename));
                        }
                    }
                }
            }
        }
        i += 1;
    }
    Ok(None)
}

/// Parse the inside of `#[nanojson(...)]`, return rename value if present.
fn parse_nanojson_attr(ts: TokenStream) -> Result<Option<String>, TokenStream> {
    let mut toks = Tokens::new(ts);
    // First ident must be "nanojson"
    match toks.peek() {
        Some(TokenTree::Ident(i)) if i.to_string() == "nanojson" => { toks.next(); }
        _ => return Ok(None), // not our attribute
    }
    // Expect `(...)`
    match toks.next() {
        Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis => {
            let mut inner = Tokens::new(g.stream());
            // parse `rename = "value"`
            while inner.peek().is_some() {
                let key = inner.next_ident()?;
                let key_str = key.to_string();
                // consume `=`
                match inner.next() {
                    Some(TokenTree::Punct(p)) if p.as_char() == '=' => {}
                    Some(other) => return compiler_error!(other, "expected `=` after `{key_str}`"),
                    None => return compiler_error!("expected `=` after `{key_str}`"),
                }
                let value = match inner.next() {
                    Some(TokenTree::Literal(lit)) => {
                        let s = lit.to_string();
                        // strip surrounding quotes
                        if s.starts_with('"') && s.ends_with('"') {
                            s[1..s.len()-1].to_string()
                        } else {
                            return compiler_error!(lit, "expected string literal for `{key_str}`");
                        }
                    }
                    Some(other) => return compiler_error!(other, "expected string literal for `{key_str}`"),
                    None => return compiler_error!("expected string literal for `{key_str}`"),
                };
                if key_str == "rename" {
                    return Ok(Some(value));
                }
                // skip comma between options
                inner.skip_ident(",");
            }
            Ok(None)
        }
        Some(other) => compiler_error!(other, "expected `(...)` after `nanojson`"),
        None => Ok(None),
    }
}

// ---- Field parsing ----

fn parse_named_fields(group: Group) -> Result<Vec<ParsedField>, TokenStream> {
    let mut toks = Tokens::new(group.stream());
    let mut fields = Vec::new();

    loop {
        // Collect leading attributes
        let mut attrs: Vec<TokenTree> = Vec::new();
        loop {
            match toks.peek() {
                Some(TokenTree::Punct(p)) if p.as_char() == '#' => {
                    attrs.push(toks.next().unwrap());
                    match toks.next() {
                        Some(t @ TokenTree::Group(_)) => attrs.push(t),
                        Some(other) => return compiler_error!(other, "expected `[...]` after `#`"),
                        None => return compiler_error!("unexpected end of input after `#`"),
                    }
                }
                _ => break,
            }
        }

        toks.skip_visibility();

        // End of fields?
        if toks.peek().is_none() { break; }

        // Field name
        let name_ident = toks.next_ident()?;
        let name_str = name_ident.to_string();
        let name_span = name_ident.span();

        // `:`
        match toks.next() {
            Some(TokenTree::Punct(p)) if p.as_char() == ':' => {}
            Some(other) => return compiler_error!(other, "expected `:` after field `{name_str}`"),
            None => return compiler_error!("expected `:` after field `{name_str}`"),
        }

        // Type tokens (up to `,`)
        let ty = toks.collect_until_comma();

        let json_name = match parse_attrs(&attrs)? {
            Some(rename) => rename,
            None => name_str.clone(),
        };

        fields.push(ParsedField { name: name_str, name_span, ty, json_name });
    }

    Ok(fields)
}

// ---- Variant parsing ----

fn parse_variants(group: Group) -> Result<Vec<ParsedVariant>, TokenStream> {
    let mut toks = Tokens::new(group.stream());
    let mut variants = Vec::new();

    loop {
        // Attributes
        let mut attrs: Vec<TokenTree> = Vec::new();
        loop {
            match toks.peek() {
                Some(TokenTree::Punct(p)) if p.as_char() == '#' => {
                    attrs.push(toks.next().unwrap());
                    match toks.next() {
                        Some(t @ TokenTree::Group(_)) => attrs.push(t),
                        Some(other) => return compiler_error!(other, "expected `[...]` after `#`"),
                        None => return compiler_error!("unexpected end of input after `#`"),
                    }
                }
                _ => break,
            }
        }

        if toks.peek().is_none() { break; }

        let name_ident = toks.next_ident()?;
        let name_str = name_ident.to_string();
        let name_span = name_ident.span();

        // Named fields `{ ... }`, tuple `( ... )`, or unit
        let fields = match toks.peek() {
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => {
                let g = match toks.next() { Some(TokenTree::Group(g)) => g, _ => unreachable!() };
                let f = parse_named_fields(g)?;
                VariantFields::Named(f)
            }
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis => {
                return compiler_error!(
                    name_ident,
                    "tuple variants are not supported by nanojson-derive (variant `{name_str}`)"
                );
            }
            _ => VariantFields::Unit,
        };

        // consume trailing comma
        if let Some(TokenTree::Punct(p)) = toks.peek() {
            if p.as_char() == ',' { toks.next(); }
        }

        let json_name = match parse_attrs(&attrs)? {
            Some(rename) => rename,
            None => name_str.clone(),
        };

        variants.push(ParsedVariant { name: name_str, name_span, fields, json_name });
    }

    Ok(variants)
}

// ---- Top-level parse ----

pub(crate) fn parse_item(input: TokenStream) -> Result<ParsedItem, TokenStream> {
    let mut toks = Tokens::new(input);

    // Skip any outer attributes (e.g. #[derive(...)])
    loop {
        match toks.peek() {
            Some(TokenTree::Punct(p)) if p.as_char() == '#' => {
                toks.next();
                toks.next(); // consume the `[...]`
            }
            _ => break,
        }
    }

    toks.skip_visibility();

    // struct or enum
    let kw = toks.next_ident()?;
    let kw_str = kw.to_string();
    if kw_str != "struct" && kw_str != "enum" {
        return compiler_error!(kw, "nanojson-derive only supports `struct` and `enum`");
    }

    let name_ident = toks.next_ident()?;
    let name_str = name_ident.to_string();
    let name_span = name_ident.span();

    // Optional generics
    let generics = toks.collect_generics()?;

    // Skip where clause if present
    if let Some(TokenTree::Ident(i)) = toks.peek() {
        if i.to_string() == "where" {
            // consume until `{`
            loop {
                match toks.peek() {
                    Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => break,
                    None => return compiler_error!("unexpected end of input in where clause"),
                    _ => { toks.next(); }
                }
            }
        }
    }

    // Body `{ ... }`
    let body = match toks.next() {
        Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => g,
        Some(other) => return compiler_error!(other, "expected `{{...}}` body"),
        None => return compiler_error!("expected `{{...}}` body"),
    };

    let kind = match kw_str.as_str() {
        "struct" => ItemKind::Struct(parse_named_fields(body)?),
        "enum"   => ItemKind::Enum(parse_variants(body)?),
        _ => unreachable!(),
    };

    Ok(ParsedItem { name: name_str, name_span, generics, kind })
}
