use proc_macro::TokenStream;
use crate::parse_item::{ItemKind, ParsedField, ParsedItem, ParsedVariant, VariantFields};

fn tts_to_string(tts: &[proc_macro::TokenTree]) -> String {
    use proc_macro::{Spacing, TokenTree};
    let mut out = String::new();
    for (i, tt) in tts.iter().enumerate() {
        // Insert a space before this token unless the *previous* token was a
        // Punct with Joint spacing (e.g. the `'` in a lifetime `'static`).
        if i > 0 {
            let joint = matches!(&tts[i - 1], TokenTree::Punct(p) if p.spacing() == Spacing::Joint);
            if !joint {
                out.push(' ');
            }
        }
        out.push_str(&tt.to_string());
    }
    out
}

/// Extract lifetime names (without the `'`) from a list of generic params.
/// e.g. `['a, T, 'b]` → `["a", "b"]`
fn lifetime_params(generics: &[proc_macro::TokenTree]) -> Vec<String> {
    use proc_macro::{Spacing, TokenTree};
    let mut lifetimes = Vec::new();
    let mut i = 0;
    while i < generics.len() {
        if let TokenTree::Punct(p) = &generics[i] {
            if p.as_char() == '\'' && p.spacing() == Spacing::Joint {
                if let Some(TokenTree::Ident(name)) = generics.get(i + 1) {
                    lifetimes.push(name.to_string());
                    i += 2;
                    continue;
                }
            }
        }
        i += 1;
    }
    lifetimes
}

fn generics_params(generics: &[proc_macro::TokenTree]) -> String {
    if generics.is_empty() {
        String::new()
    } else {
        format!("<{}>", tts_to_string(generics))
    }
}

// ---- Serialize codegen ----

pub(crate) fn gen_serialize(item: &ParsedItem) -> Result<TokenStream, TokenStream> {
    let name = &item.name;
    let gp = generics_params(&item.generics);

    let body = match &item.kind {
        ItemKind::Struct(fields) => gen_serialize_fields(fields, "self")?,
        ItemKind::Enum(variants) => gen_serialize_enum(variants)?,
    };

    let code = format!(
        r#"
        impl{gp} ::nanojson::Serialize for {name}{gp} {{
            fn serialize<__W: ::nanojson::Write>(
                &self,
                __json: &mut ::nanojson::Serializer<__W>,
            ) -> ::core::result::Result<(), ::nanojson::SerializeError<__W::Error>> {{
                {body}
                ::core::result::Result::Ok(())
            }}
        }}
        "#
    );

    code.parse().map_err(|e| {
        crate::helpers::emit_compilation_error(
            &format!("nanojson-derive internal error generating Serialize: {e}"),
            &proc_macro::Span::call_site(),
        )
    })
}

fn gen_serialize_fields(fields: &[ParsedField], self_expr: &str) -> Result<String, TokenStream> {
    let mut stmts = String::new();
    stmts.push_str("__json.object_begin()?;");
    for f in fields {
        let fname = &f.name;
        let jname = escape_rust_str(&f.json_name);
        stmts.push_str(&format!(
            "__json.member({jname})?; \
             ::nanojson::Serialize::serialize(&{self_expr}.{fname}, __json)?;"
        ));
    }
    stmts.push_str("__json.object_end()?;");
    Ok(stmts)
}

fn gen_serialize_enum(variants: &[ParsedVariant]) -> Result<String, TokenStream> {
    let mut arms = String::new();
    for v in variants {
        let vname = &v.name;
        let jname = escape_rust_str(&v.json_name);
        let arm = match &v.fields {
            // Unit variants always serialize as a plain JSON string, whether the
            // enum is pure-unit or mixed. The deserializer accepts both this form
            // and the {"Variant": null} object form for maximum compatibility.
            VariantFields::Unit => {
                format!("Self::{vname} => {{ __json.string({jname})?; }}")
            }
            VariantFields::Named(fields) => {
                // Pattern: Self::Variant { field1, field2, ... }
                let pat_fields: String = fields.iter()
                    .map(|f| format!("{}, ", f.name))
                    .collect();
                let mut body = String::new();
                body.push_str("__json.object_begin()?;");
                body.push_str(&format!("__json.member({jname})?;"));
                body.push_str("__json.object_begin()?;");
                for f in fields {
                    let fname = &f.name;
                    let fjname = escape_rust_str(&f.json_name);
                    body.push_str(&format!(
                        "__json.member({fjname})?; \
                         ::nanojson::Serialize::serialize({fname}, __json)?;"
                    ));
                }
                body.push_str("__json.object_end()?;");
                body.push_str("__json.object_end()?;");
                format!("Self::{vname} {{ {pat_fields} }} => {{ {body} }}")
            }
        };
        arms.push_str(&arm);
    }
    Ok(format!("match self {{ {arms} }}"))
}

// ---- Deserialize codegen ----

pub(crate) fn gen_deserialize(item: &ParsedItem) -> Result<TokenStream, TokenStream> {
    let name = &item.name;
    let gp = generics_params(&item.generics);

    let body = match &item.kind {
        ItemKind::Struct(fields) => gen_deserialize_object_fields(fields, name, name, true)?,
        ItemKind::Enum(variants) => gen_deserialize_enum(name, variants)?,
    };

    // For any lifetime params in the struct generics (e.g. `'a` in `Foo<'a>`),
    // add a `'__src: 'lt` bound so that &'lt str fields can be filled from the
    // source buffer via current_string_src().
    let lifetimes = lifetime_params(&item.generics);
    let where_clause = if lifetimes.is_empty() {
        String::new()
    } else {
        let bounds: String = lifetimes.iter()
            .map(|lt| format!("'__src: '{lt}, "))
            .collect();
        format!("where {bounds}")
    };

    let code = format!(
        r#"
        impl<'__src{comma_gp}> ::nanojson::Deserialize<'__src>
            for {name}{gp}
        {where_clause}
        {{
            fn deserialize<'__buf>(
                __json: &mut ::nanojson::Parser<'__src, '__buf>,
                ) -> ::core::result::Result<Self, ::nanojson::ParseError> {{
                {body}
            }}
        }}
        "#,
        comma_gp = if item.generics.is_empty() {
            String::new()
        } else {
            format!(", {}", tts_to_string(&item.generics))
        },
    );

    code.parse().map_err(|e| {
        crate::helpers::emit_compilation_error(
            &format!("nanojson-derive internal error generating Deserialize: {e}"),
            &proc_macro::Span::call_site(),
        )
    })
}

/// Shared core for deserializing a JSON object into named fields.
///
/// `constructor` is the Rust path used to build the value: a struct name like `"Point"` or an
/// enum variant path like `"Event::Login"`.  When `wrap_ok` is true the result expression is
/// wrapped in `::core::result::Result::Ok(...)` (used by top-level struct impls); for enum
/// variants the caller is responsible for the outer `Ok`.
fn gen_deserialize_object_fields(
    fields: &[ParsedField],
    constructor: &str,
    type_name: &str,
    wrap_ok: bool,
) -> Result<String, TokenStream> {
    let mut code = String::new();

    for f in fields {
        let fname = &f.name;
        let fty = tts_to_string(&f.ty);
        code.push_str(&format!(
            "let mut {fname}: ::core::option::Option<{fty}> = ::core::option::Option::None;"
        ));
    }

    // Build a static slice literal of valid JSON field names for error reporting.
    let field_names: String = fields.iter()
        .map(|f| format!("{}, ", escape_rust_str(&f.json_name)))
        .collect();
    let expected_fields_expr = format!("&[{field_names}]");

    code.push_str("__json.object_begin()?;");
    code.push_str("while let ::core::option::Option::Some(__key) = __json.member()? {");
    code.push_str("match __key {");
    for f in fields {
        let fname = &f.name;
        let jname = escape_rust_str(&f.json_name);
        code.push_str(&format!(
            "{jname} => {{ {fname} = ::core::option::Option::Some(\
                ::nanojson::Deserialize::deserialize(__json)?\
            ); }}"
        ));
    }
    let tn = escape_rust_str(type_name);
    code.push_str(&format!(
        "_ => {{ return ::core::result::Result::Err(\
            __json.unknown_field_in({tn}, {expected_fields_expr})\
        ); }}"
    ));
    code.push_str("}"); // match
    code.push_str("}"); // while
    code.push_str("__json.object_end()?;");

    if wrap_ok {
        code.push_str(&format!("::core::result::Result::Ok({constructor} {{"));
    } else {
        code.push_str(&format!("{constructor} {{"));
    }
    for f in fields {
        let fname = &f.name;
        let jname = escape_rust_str(&f.json_name);
        if f.has_default {
            code.push_str(&format!(
                "{fname}: {fname}.unwrap_or(::core::default::Default::default()),"
            ));
        } else {
            code.push_str(&format!(
                "{fname}: {fname}.ok_or_else(|| ::nanojson::ParseError {{ \
                    kind: ::nanojson::ParseErrorKind::MissingField {{ field: {jname} }}, \
                    offset: __json.error_offset() \
                }})?,"
            ));
        }
    }
    code.push_str(if wrap_ok { "})" } else { "}" });

    Ok(code)
}

fn gen_deserialize_enum(name: &str, variants: &[ParsedVariant]) -> Result<String, TokenStream> {
    let mut code = String::new();

    // For unit enums, read a string and match it.
    // For enums with struct variants, read an object with one key.
    let all_unit = variants.iter().all(|v| matches!(v.fields, VariantFields::Unit));

    // Static slice of all variant JSON names for error reporting.
    let variant_names: String = variants.iter()
        .map(|v| format!("{}, ", escape_rust_str(&v.json_name)))
        .collect();
    let expected_variants_expr = format!("&[{variant_names}]");
    let tn = escape_rust_str(name);

    if all_unit {
        code.push_str("let __tag = __json.string()?;");
        code.push_str("match __tag {");
        for v in variants {
            let vname = &v.name;
            let jname = escape_rust_str(&v.json_name);
            code.push_str(&format!(
                "{jname} => ::core::result::Result::Ok({name}::{vname}),"
            ));
        }
        code.push_str(&format!(
            "_ => ::core::result::Result::Err(\
                __json.unknown_field_in({tn}, {expected_variants_expr})\
            ),"
        ));
        code.push_str("}"); // match
    } else {
        // Mixed enum: unit variants serialize as plain strings; struct variants use
        // externally-tagged objects.  The deserializer accepts both:
        //   "Logout"            – the canonical form produced by nanojson
        //   {"Logout": null}    – alternative object form (for interop / hand-written JSON)

        // ---- string path: only unit variants are valid here ----
        code.push_str("if __json.is_string_ahead() {");
        code.push_str("let __tag = __json.string()?;");
        code.push_str("match __tag {");
        for v in variants {
            if matches!(v.fields, VariantFields::Unit) {
                let vname = &v.name;
                let jname = escape_rust_str(&v.json_name);
                code.push_str(&format!(
                    "{jname} => ::core::result::Result::Ok({name}::{vname}),"
                ));
            }
        }
        // Struct variants named as a plain string are a type mismatch.
        for v in variants {
            if matches!(v.fields, VariantFields::Named(_)) {
                let jname = escape_rust_str(&v.json_name);
                code.push_str(&format!(
                    "{jname} => return ::core::result::Result::Err(::nanojson::ParseError {{ \
                        kind: ::nanojson::ParseErrorKind::UnexpectedToken {{ \
                            expected: \"object\", got: \"string\" \
                        }}, \
                        offset: __json.error_offset() \
                    }}),"
                ));
            }
        }
        code.push_str(&format!(
            "_ => return ::core::result::Result::Err(\
                __json.unknown_field_in({tn}, {expected_variants_expr})\
            ),"
        ));
        code.push_str("}"); // match

        // ---- object path: struct variants + optional {"Unit": null} ----
        code.push_str("} else {");
        code.push_str("__json.object_begin()?;");
        code.push_str("let __result = if let ::core::option::Option::Some(__tag) = __json.member()? {");
        code.push_str("match __tag {");
        for v in variants {
            let vname = &v.name;
            let jname = escape_rust_str(&v.json_name);
            let inner = match &v.fields {
                VariantFields::Unit => {
                    format!("{{ __json.null()?; {name}::{vname} }}")
                }
                VariantFields::Named(fields) => {
                    let body = gen_deserialize_object_fields(
                        fields,
                        &format!("{name}::{vname}"),
                        name,
                        false,
                    )?;
                    format!("{{ {body} }}")
                }
            };
            code.push_str(&format!("{jname} => {inner},"));
        }
        code.push_str(&format!(
            "_ => return ::core::result::Result::Err(\
                __json.unknown_field_in({tn}, {expected_variants_expr})\
            ),"
        ));
        code.push_str("}"); // match
        code.push_str("} else {");
        code.push_str(&format!(
            "return ::core::result::Result::Err(::nanojson::ParseError {{ \
                kind: ::nanojson::ParseErrorKind::MissingField {{ field: \"(variant)\" }}, \
                offset: __json.error_offset() \
            }});"
        ));
        code.push_str("};"); // if let
        code.push_str("__json.object_end()?;");
        code.push_str("::core::result::Result::Ok(__result)");
        code.push_str("}"); // else (object path)
    }

    Ok(code)
}

// ---- helpers ----

/// Produce a Rust string literal for a JSON key name.
fn escape_rust_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"'  => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c    => out.push(c),
        }
    }
    out.push('"');
    out
}
