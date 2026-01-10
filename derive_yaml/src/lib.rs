extern crate proc_macro;

use proc_macro::{Delimiter, TokenStream, TokenTree};

#[proc_macro_derive(FromYaml)]
pub fn derive_from_yaml(input: TokenStream) -> TokenStream {
    let struct_name = match extract_struct_name(input.clone()) {
        Some(name) => name,
        None => return quote_error("Failed to extract struct name"),
    };

    let fields = match extract_struct_fields(input) {
        Some(f) => f,
        None => return quote_error("Failed to extract struct fields"),
    };

    let arms = generate_match_arms(&fields);
    let flags = generate_field_flags(&fields);

    let code = format_impl_code(&struct_name, &flags, &arms);

    code.parse().unwrap_or_else(|_| quote_error("Generated code was invalid"))
}

// ====== Field Extraction ======

fn extract_struct_name(input: TokenStream) -> Option<String> {
    let mut tokens = input.into_iter();

    while let Some(token) = tokens.next() {
        if let TokenTree::Ident(ident) = token {
            if ident.to_string() == "struct" {
                if let Some(TokenTree::Ident(name)) = tokens.next() {
                    return Some(name.to_string());
                }
            }
        }
    }

    None
}

fn extract_struct_fields(input: TokenStream) -> Option<Vec<String>> {
    let tokens: Vec<TokenTree> = input.into_iter().collect();
    let mut fields = Vec::new();

    // Find the opening brace
    for token in tokens.iter() {
        if let TokenTree::Group(group) = token {
            if group.delimiter() == Delimiter::Brace {
                parse_field_names(group.stream(), &mut fields);
                return Some(fields);
            }
        }
    }

    None
}

fn parse_field_names(group_stream: TokenStream, fields: &mut Vec<String>) {
    let mut group_iter = group_stream.into_iter();
    let mut last_ident = String::new();

    while let Some(inner_token) = group_iter.next() {
        match inner_token {
            TokenTree::Ident(ident) => {
                let s = ident.to_string();
                // Skip keywords and type hints
                if !is_keyword_or_type(&s) {
                    last_ident = s;
                }
            }
            TokenTree::Punct(punct) => {
                if punct.as_char() == ':' {
                    if !last_ident.is_empty() {
                        fields.push(last_ident.clone());
                        last_ident.clear();
                    }
                    // Skip until comma
                    skip_to_comma(&mut group_iter);
                }
            }
            _ => {}
        }
    }
}

fn is_keyword_or_type(s: &str) -> bool {
    matches!(s, "pub" | "ConfigParser" | "ParseResult" | "FromYaml")
}

fn skip_to_comma(iter: &mut impl Iterator<Item = TokenTree>) {
    while let Some(token) = iter.next() {
        if let TokenTree::Punct(p) = token {
            if p.as_char() == ',' {
                break;
            }
        }
    }
}

// ====== Code Generation ======

fn generate_field_flags(fields: &[String]) -> String {
    let mut flags = String::new();

    for field in fields {
        flags.push_str(&format!("let mut seen_{} = false;\n", field));
    }

    flags
}

fn generate_match_arms(fields: &[String]) -> String {
    let mut arms = String::new();

    for field in fields {
        arms.push_str(&format!(
            r#"{q}{field}{q} => {{
    if seen_{field} {{
        return Err(crate::config::ConfigError {{
            message: format!({q}Duplicate field '{field}'{q}),
            loc: parser.peek_loc(),
            context: vec![]
        }});
    }}
    seen_{field} = true;
    parser.consume_key(&key)?;
    obj.{field} = FromYaml::from_yaml(parser, min_indent)
        .map_err(|mut e| {{ e.context.push(format!({q}parsing field '{field}'{q})); e }})?;
}},
"#,
            field = field,
            q = "\""
        ));
    }

    arms
}

fn format_impl_code(struct_name: &str, flags: &str, arms: &str) -> String {
    format!(
        r#"impl FromYaml for {struct_name} {{
    fn from_yaml(parser: &mut crate::config::ConfigParser, min_indent: usize) -> crate::config::ParseResult<Self> {{
        let mut obj = Self::default();
        let mut struct_indent: Option<usize> = None;
        {flags}
        loop {{
            if !parser.check_indentation(min_indent, &mut struct_indent)? {{
                break;
            }}
            if parser.is_end_of_block() {{
                break;
            }}
            let key = match parser.parse_map_key()? {{
                Some(k) => k,
                None => break,
            }};

            match key.as_str() {{
                {arms}
                _ => {{
                    eprintln!("Warning: Unknown field {{}}", key);
                    parser.consume_key(&key)?;
                    parser.skip_value(struct_indent.unwrap_or(min_indent))?;
                }}
            }}
        }}
        Ok(obj)
    }}
}}"#,
        struct_name = struct_name,
        flags = flags,
        arms = arms
    )
}

// ====== Error Handling ======

fn quote_error(msg: &str) -> TokenStream {
    format!(
        "compile_error!(\"FromYaml derive error: {}\");",
        msg
    )
    .parse()
    .unwrap()
}