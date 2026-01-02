use proc_macro::{Delimiter, TokenStream, TokenTree};

#[proc_macro_derive(YamlStruct, attributes(parcast))]
pub fn derive_yaml_struct(input: TokenStream) -> TokenStream {
    let tokens: Vec<TokenTree> = input.into_iter().collect();
    let mut struct_name = String::new();
    let mut fields = Vec::new();
    let mut pending_default = None;
    let mut rename = None;
    let mut skip = false;

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
                if let TokenTree::Punct(ref p) = inner[j]
                    && p.as_char() == '#'
                {
                    if let TokenTree::Group(g) = &inner[j + 1]
                        && g.delimiter() == Delimiter::Bracket
                    {
                        let attr_tokens: Vec<TokenTree> = g.stream().into_iter().collect();
                        if attr_tokens.len() >= 2
                            && let TokenTree::Ident(ref attr_ident) = attr_tokens[0]
                            && attr_ident.to_string() == "parcast"
                            && let TokenTree::Group(ref attr_group) = attr_tokens[1]
                            && attr_group.delimiter() == Delimiter::Parenthesis
                        {
                            let attr_inner: Vec<TokenTree> =
                                attr_group.stream().into_iter().collect();
                            for k in 0..attr_inner.len() {
                                match attr_inner[k] {
                                    TokenTree::Ident(ref key_ident)
                                        if key_ident.to_string() == "default" =>
                                    {
                                        if let TokenTree::Punct(ref eq_punct) = attr_inner[k + 1]
                                            && eq_punct.as_char() == '='
                                            && let TokenTree::Literal(ref lit) = attr_inner[k + 2]
                                        {
                                            pending_default = Some(lit.to_string());
                                        }
                                    }
                                    TokenTree::Ident(ref key_ident)
                                        if key_ident.to_string() == "rename" =>
                                    {
                                        if let TokenTree::Punct(ref eq_punct) = attr_inner[k + 1]
                                            && eq_punct.as_char() == '='
                                            && let TokenTree::Literal(ref lit) = attr_inner[k + 2]
                                        {
                                            rename = Some(lit.to_string())
                                        }
                                    }
                                    TokenTree::Ident(ref key_ident)
                                        if key_ident.to_string() == "skip" =>
                                    {
                                        skip = true
                                    }

                                    _ => {}
                                }
                                // if let TokenTree::Ident(ref key_ident) = attr_inner[k]
                                //     && key_ident.to_string() == "default"
                                //     && let TokenTree::Punct(ref eq_punct) = attr_inner[k + 1]
                                //     && eq_punct.as_char() == '='
                                //     && let TokenTree::Literal(ref lit) = attr_inner[k + 2]
                                // {
                                //     pending_default = Some(lit.to_string());
                                // }
                            }
                        }
                    }
                    continue;
                }

                if let TokenTree::Punct(ref p) = inner[j]
                    && p.as_char() == ':'
                    && j > 0
                    && let TokenTree::Ident(ref field_ident) = inner[j - 1]
                {
                    if skip {
                        skip = false;
                        continue;
                    }

                    let field_name = field_ident.to_string();
                    let yaml_key_name = rename.take().map(|s| s.trim_matches('"').to_string()).unwrap_or(field_name.clone());

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

                        fields.push((field_name, is_option, yaml_key_name , pending_default.take()));
                    }
                }
            }
        }
    }

    // 2. Generate FromYaml impl
    let mut generated = format!(
        "impl parser::FromYaml for {name} {{
            fn from_yaml(value: &parser::YamlValue) -> std::result::Result<Self, parser::YamlError> {{
                if let parser::YamlValue::Map(m) = value {{
                    let known_fields = vec![{fields}];
                    for key in m.keys() {{
                        if !known_fields.contains(&key.to_string().as_str()) {{
                            println!(\"\\x1b[1;33mWarning:\\x1b[0m Unknown field '{{}}' found in {name} configuration. Skipping it.\", key);
                        }}
                    }}
                    std::result::Result::Ok(Self {{",
        name = struct_name, fields = fields
            .iter()
            .map(|(_, _, f, _)| format!("\"{}\"", f))
            .collect::<Vec<String>>()
            .join(", ")
    );

    for (field, is_option,yaml_key, default_value) in fields {
        if is_option {
            generated.push_str(&format!(
                "{field}: parser::FromYaml::from_yaml_opt(m.get(\"{yaml_key}\"), \"{field}\")?,",
                field = field
            ));
        } else if let Some(def) = default_value {
            let clean_def = def.trim_matches('"');
            generated.push_str(&format!(
                "{field}: match m.get(\"{yaml_key}\") {{
                            Some(v) => parser::FromYaml::from_yaml(v)?, 
                            None => {{
                                // Reuse your actual Parser here!
                                let mut p = parser::Parser::new(\"{clean_def}\")?;
                                let default_yaml = p.parse()?;
                                parser::FromYaml::from_yaml(&default_yaml)?
                            }}
                        }},",
                field = field,
                clean_def = clean_def
            ));
        } else {
            // Required field logic
            generated.push_str(&format!(
            "{field}: parser::FromYaml::from_yaml(m.get(\"{yaml_key}\").ok_or_else(||  parser::YamlError::Generic(\"Missing required field: {field}\".into()))?)?,",
            field = field
        ));
        }
    }

    generated.push_str("..std::default::Default::default() ");
    generated.push_str("}) } else { std::result::Result::Err(parser::YamlError::Generic(\"Expected a Map\".into())) } } }");

    generated.parse().expect("Generated code was invalid")
}
