use std::collections::BTreeMap;

/// A YAML value — enough to represent GitHub Actions workflow structure.
#[derive(Debug, Clone, PartialEq)]
pub enum YamlValue {
    Null,
    Bool(bool),
    Integer(i64),
    String(String),
    Sequence(Vec<YamlValue>),
    Mapping(BTreeMap<String, YamlValue>),
}

impl YamlValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            YamlValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            YamlValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_mapping(&self) -> Option<&BTreeMap<String, YamlValue>> {
        match self {
            YamlValue::Mapping(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_sequence(&self) -> Option<&[YamlValue]> {
        match self {
            YamlValue::Sequence(s) => Some(s),
            _ => None,
        }
    }

    /// Get a value by key (mapping only).
    pub fn get(&self, key: &str) -> Option<&YamlValue> {
        self.as_mapping()?.get(key)
    }

    /// Navigate a dotted path like "jobs.build.steps".
    pub fn get_path(&self, path: &str) -> Option<&YamlValue> {
        let mut current = self;
        for key in path.split('.') {
            current = current.get(key)?;
        }
        Some(current)
    }

    /// Get keys of a mapping.
    pub fn keys(&self) -> Vec<&str> {
        match self {
            YamlValue::Mapping(m) => m.keys().map(|k| k.as_str()).collect(),
            _ => vec![],
        }
    }
}
