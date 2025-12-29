use std::collections::HashMap;

use crate::YamlValue;

pub trait FromYaml: Sized {
    fn from_yaml(value: &YamlValue) -> std::result::Result<Self, String>;

    // This MUST exist for the macro to work
    fn from_yaml_opt(value: Option<&YamlValue>, name: &str) -> std::result::Result<Self, String> {
        match value {
            Some(v) => Self::from_yaml(v),
            None => Self::from_yaml(&YamlValue::Scalar(""))
                .map_err(|_| format!("Missing required field: {}", name)),
        }
    }
}

impl FromYaml for String {
    fn from_yaml(value: &YamlValue) -> Result<Self, String> {
        match value {
            YamlValue::Scalar(s) => Ok(s.to_string()),
            _ => Err("Expected string".into()),
        }
    }
}

impl FromYaml for bool {
    fn from_yaml(v: &YamlValue) -> std::result::Result<Self, String> {
        match v {
            YamlValue::Scalar("true") | YamlValue::Scalar("on") => Ok(true),
            YamlValue::Scalar("false") | YamlValue::Scalar("off") | YamlValue::Scalar("") => {
                Ok(false)
            }
            _ => Err("Invalid boolean".into()),
        }
    }
}

impl<T: FromYaml> FromYaml for Vec<T> {
    fn from_yaml(value: &YamlValue) -> std::result::Result<Self, String> {
        if let YamlValue::List(items) = value {
            items.iter().map(|i| T::from_yaml(i)).collect()
        } else {
            // Support "auto-coaxing": if it's a single value, turn it into a Vec of one
            Ok(vec![T::from_yaml(value)?])
        }
    }
}

impl<T: FromYaml> FromYaml for Option<T> {
    fn from_yaml(v: &YamlValue) -> std::result::Result<Self, String> {
        match v {
            YamlValue::Scalar("") => Ok(None),
            _ => T::from_yaml(v).map(Some),
        }
    }
}

impl<K, V> FromYaml for std::collections::HashMap<K, V>
where
    K: std::str::FromStr + std::hash::Hash + Eq,
    V: FromYaml,
    K::Err: std::fmt::Display,
{
    fn from_yaml(value: &YamlValue) -> std::result::Result<Self, String> {
        if let YamlValue::Map(m) = value {
            let mut map = std::collections::HashMap::new();
            for (k_str, v) in m {
                let key = k_str.parse::<K>().map_err(|e| e.to_string())?;
                let val = V::from_yaml(v)?;
                map.insert(key, val);
            }
            Ok(map)
        } else {
            Err("Expected a Map".into())
        }
    }
}

macro_rules! impl_from_yaml_numeric {
    ($($t:ty),*) => {
        $(
            impl FromYaml for $t {
                fn from_yaml(v: &YamlValue) -> std::result::Result<Self, String> {
                    match v {
                        YamlValue::Scalar(s) => s.parse::<$t>().map_err(|e| e.to_string()),
                        _ => Err(format!("Expected a numeric scalar for {}", stringify!($t))),
                    }
                }
            }
        )*
    };
}

// Now apply it to every number type you need
impl_from_yaml_numeric!(u16, u32, u64, usize, i32, i64, f64);
