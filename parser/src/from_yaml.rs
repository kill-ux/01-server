

use crate::{YamlError, YamlValue};

pub trait FromYaml: Sized {
    fn from_str(source: &str) -> std::result::Result<Self, YamlError> {
        let mut parser = crate::Parser::new(source)?;
        let yaml_value = parser.parse()?;
        Self::from_yaml(&yaml_value)
    }

    fn from_yaml(value: &YamlValue) -> std::result::Result<Self, YamlError>;

    fn from_yaml_opt(value: Option<&YamlValue>, name: &str) -> std::result::Result<Self, YamlError> {
        match value {
            Some(v) => Self::from_yaml(v),
            None => Err(YamlError::Generic(format!("Missing required field: {}", name))),
        }
    }
}

impl FromYaml for String {
    fn from_yaml(value: &YamlValue) -> Result<Self, YamlError> {
        match value {
            YamlValue::Scalar(s) => Ok(s.to_string()),
            _ => Err(YamlError::Generic("Expected string".into())),
        }
    }
}


// impl FromYaml for IpAddr {
//     fn from_yaml(value: &YamlValue) -> std::result::Result<Self, YamlError> {
//         match value {
//             YamlValue::Scalar(s) => s.parse().map_err(YamlError::Generic(|ep| format!("{ep}"))),
//             _ => {},
//         }
//     }
// }


impl FromYaml for bool {
    fn from_yaml(v: &YamlValue) -> std::result::Result<Self, YamlError> {
        match v {
            YamlValue::Scalar("true") | YamlValue::Scalar("on") => Ok(true),
            YamlValue::Scalar("false") | YamlValue::Scalar("off") | YamlValue::Scalar("") => {
                Ok(false)
            }
            _ => Err(YamlError::Generic("Invalid boolean".into())),
        }
    }
}

impl<T: FromYaml> FromYaml for Vec<T> {
    fn from_yaml(value: &YamlValue) -> std::result::Result<Self, YamlError> {
        if let YamlValue::List(items) = value {
            items.iter().map(|i| T::from_yaml(i)).collect()
        } else {
            Ok(vec![T::from_yaml(value)?])
        }
    }
}

impl<T: FromYaml> FromYaml for Option<T> {
    fn from_yaml(value: &YamlValue) -> Result<Self, YamlError> {
        T::from_yaml(value).map(Some)
    }

    fn from_yaml_opt(value: Option<&YamlValue>, _name: &str) -> Result<Self, YamlError> {
        match value {
            Some(v) => Self::from_yaml(v),
            None => Ok(None),
        }
    }
}

impl<K, V> FromYaml for std::collections::HashMap<K, V>
where
    K: std::str::FromStr + std::hash::Hash + Eq,
    V: FromYaml,
    K::Err: std::fmt::Display,
{
    fn from_yaml(value: &YamlValue) -> std::result::Result<Self, YamlError> {
        match value {
            YamlValue::Map(m) => {
                let mut map = std::collections::HashMap::new();
                for (k_str, v) in m {
                    let key = k_str.parse::<K>().map_err(|e| YamlError::Generic(e.to_string()))?;
                    let val = V::from_yaml(v)?;
                    map.insert(key, val);
                }
                Ok(map)
            }
            _ => Err(YamlError::Generic("Expected a Map".into())),
        }
    }
}

macro_rules! impl_from_yaml_numeric {
    ($($t:ty),*) => {
        $(
            impl FromYaml for $t {
                fn from_yaml(v: &YamlValue) -> std::result::Result<Self, YamlError> {
                    match v {
                        YamlValue::Scalar(s) => s.parse::<$t>().map_err(|e| YamlError::Generic(format!("{} | {s}", e))),
                        _ => Err(YamlError::Generic(format!("Expected a numeric scalar for {}", stringify!($t)))),
                    }
                }
            }
        )*
    };
}

impl_from_yaml_numeric!(u16, u32, u64, usize, i32, i64, f64);
