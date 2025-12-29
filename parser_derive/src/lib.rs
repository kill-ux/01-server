extern crate proc_macro;
use proc_macro::{Delimiter, TokenStream, TokenTree};

#[proc_macro_derive(YamlStruct)]
pub fn derive_yaml_struct(input: TokenStream) -> TokenStream {
    let tokens: Vec<TokenTree> = input.into_iter().collect();
    let mut struct_name = String::new();
    let mut fields = Vec::new();

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

                        fields.push((field_name, is_option));
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

    for (field, is_option) in fields {
        if is_option {
            generated.push_str(&format!(
                "{field}: parser::FromYaml::from_yaml_opt(m.get(\"{field}\"), \"{field}\")?,",
                field = field
            ));
        } else {
            generated.push_str(&format!(
            "{field}: parser::FromYaml::from_yaml(m.get(\"{field}\").ok_or_else(|| std::string::String::from(\"Missing required field: {field}\"))?)?,",
            field = field
        ));
        }
    }

    generated.push_str("}) } else { std::result::Result::Err(::std::string::String::from(\"Expected a Map\")) } } }");

    generated.parse().expect("Generated code was invalid")
}
