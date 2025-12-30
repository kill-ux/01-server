extern crate proc_macro;
// use parser::FromYaml;
use proc_macro::{Delimiter, TokenStream, TokenTree};

#[proc_macro_derive(YamlStruct, attributes(field))]
pub fn derive_yaml_struct(input: TokenStream) -> TokenStream {
    let tokens: Vec<TokenTree> = input.into_iter().collect();
    let mut struct_name = String::new();
    let mut fields = Vec::new();
    let mut pending_default = None;

    // 1. Identify Struct Name and Fields
    for i in 0..tokens.len() {
        if let TokenTree::Ident(ref ident) = tokens[i]
            && ident.to_string() == "struct"
            && let Some(TokenTree::Ident(name)) = tokens.get(i + 1)
        {
            struct_name = name.to_string();
        }

        if let TokenTree::Group(ref group) = tokens[i]
            && group.delimiter() == Delimiter::Brace
        {
            let inner: Vec<TokenTree> = group.stream().into_iter().collect();
            for j in 0..inner.len() {
                if let TokenTree::Punct(ref p) = inner[j] {
                    if p.as_char() == '#' {
                        if let TokenTree::Group(g) = &inner[j + 1]
                            && g.delimiter() == Delimiter::Bracket
                        {
                            let attr_tokens: Vec<TokenTree> = g.stream().into_iter().collect();
                            if attr_tokens.len() >= 2 {
                                if let TokenTree::Ident(ref attr_ident) = attr_tokens[0]
                                    && attr_ident.to_string() == "field"
                                    && let TokenTree::Group(ref attr_group) = attr_tokens[1]
                                    && attr_group.delimiter() == Delimiter::Parenthesis
                                {
                                    let attr_inner: Vec<TokenTree> =
                                        attr_group.stream().into_iter().collect();
                                    for k in 0..attr_inner.len() {
                                        if let TokenTree::Ident(ref key_ident) = attr_inner[k]
                                            && key_ident.to_string() == "default"
                                            && let TokenTree::Punct(ref eq_punct) =
                                                attr_inner[k + 1]
                                            && eq_punct.as_char() == '='
                                        {
                                            if let TokenTree::Literal(ref lit) = attr_inner[k + 2] {
                                                pending_default = Some(lit.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        continue;
                    }
                }

                if let TokenTree::Punct(ref p) = inner[j]
                    && p.as_char() == ':'
                    && j > 0
                    && let TokenTree::Ident(ref field_ident) = inner[j - 1]
                {
                    let field_name = field_ident.to_string();

                    if field_name != "pub" && field_name != "crate" {
                        // Look ahead to see if 'Option' appears before the next comma
                        let mut is_option = false;
                        let mut k = j + 1;
                        while k < inner.len() {
                            let token_str = inner[k].to_string();
                            if token_str == "," {
                                break;
                            } // End of field
                            if token_str == "Option" {
                                is_option = true;
                                break;
                            }
                            k += 1;
                        }

                        fields.push((field_name, is_option, pending_default.take()));
                    }
                }
            }
        }
    }

    // 2. Generate FromYaml impl
    let mut generated = format!(
        "impl parser::FromYaml for {name} {{
            fn from_yaml(value: &parser::YamlValue) -> std::result::Result<Self, std::string::String> {{
                if let parser::YamlValue::Map(m) = value {{
                    std::result::Result::Ok(Self {{",
        name = struct_name
    );
    for (field, is_option, default_value) in fields {
        if is_option {
            generated.push_str(&format!(
                "{field}: parser::FromYaml::from_yaml_opt(m.get(\"{field}\"), \"{field}\")?,",
                field = field
            ));
        } else if let Some(def) = default_value {
            let clean_def = def.trim_matches('"');
            generated.push_str(&format!(
                "{field}: match m.get(\"{field}\") {{
                            Some(v) => parser::FromYaml::from_yaml(v)?, 
                            None => {{
                                // Reuse your actual Parser here!
                                let mut p = parser::Parser::new(\"{clean_def}\").map_err(|e| e.to_string())?;
                                let default_yaml = p.parse().map_err(|e| e.to_string())?;
                                parser::FromYaml::from_yaml(&default_yaml)?
                            }}
                        }},",
                field = field,
                clean_def = clean_def
            ));
        } else {
            // Required field logic
            generated.push_str(&format!(
            "{field}: parser::FromYaml::from_yaml(m.get(\"{field}\").ok_or_else(|| std::string::String::from(\"Missing required field: {field}\"))?)?,",
            field = field
        ));
        }
    }

    generated.push_str("}) } else { std::result::Result::Err(::std::string::String::from(\"Expected a Map\")) } } }");

    generated.parse().expect("Generated code was invalid")
}
