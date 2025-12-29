extern crate proc_macro;
use proc_macro::{Delimiter, TokenStream, TokenTree};

#[proc_macro_derive(YamlStruct)]
pub fn derive_yaml_struct(input: TokenStream) -> TokenStream {
    let tokens: Vec<TokenTree> = input.into_iter().collect();
    let mut struct_name = String::new();
    let mut fields = Vec::new();

    // 1. Identify Struct Name and Fields
    for i in 0..tokens.len() {
        if let TokenTree::Ident(ref ident) = tokens[i] {
            if ident.to_string() == "struct" {
                if let Some(TokenTree::Ident(name)) = tokens.get(i + 1) {
                    struct_name = name.to_string();
                }
            }
        }

        if let TokenTree::Group(ref group) = tokens[i] {
            if group.delimiter() == Delimiter::Brace {
                let inner: Vec<TokenTree> = group.stream().into_iter().collect();
                for j in 0..inner.len() {
                    if let TokenTree::Punct(ref p) = inner[j] {
                        // We found the colon!
                        if p.as_char() == ':' && j > 0 {
                            // Grab ONLY the identifier immediately to the left
                            if let TokenTree::Ident(ref field_ident) = inner[j - 1] {
                                let field_name = field_ident.to_string();
                                // Double check it's not a keyword
                                if field_name != "pub" && field_name != "crate" {
                                    fields.push(field_name);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Generate the implementation
    // We use absolute paths (::std) to prevent clashes with your local Result type
    let mut generated = format!(
        "impl ::parser::FromYaml for {name} {{
            fn from_yaml(value: &::parser::YamlValue) -> ::std::result::Result<Self, ::std::string::String> {{
                if let ::parser::YamlValue::Map(m) = value {{
                    ::std::result::Result::Ok(Self {{",
        name = struct_name
    );

    for field in fields {
        generated.push_str(&format!(
            "{field}: ::parser::FromYaml::from_yaml_opt(m.get(\"{field}\"), \"{field}\")?,",
            field = field
        ));
    }

    generated.push_str("}) } else { ::std::result::Result::Err(::std::string::String::from(\"Expected a Map\")) } } }");

    println!("DEBUG GENERATED: {}", generated);

    generated.parse().expect("Generated code was invalid")
}
