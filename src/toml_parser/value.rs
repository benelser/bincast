use std::collections::BTreeMap;
use std::fmt;

/// A TOML value. Covers the subset we need: strings, integers, booleans,
/// arrays, and tables. No datetime support.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Array(Vec<Value>),
    Table(BTreeMap<String, Value>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Value::Integer(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_table(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Table(t) => Some(t),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_table()?.get(key)
    }

    /// Navigate a dotted path like "package.name"
    pub fn get_path(&self, path: &str) -> Option<&Value> {
        let mut current = self;
        for key in path.split('.') {
            current = current.get(key)?;
        }
        Some(current)
    }

    /// Get a string at a dotted path
    pub fn get_str(&self, path: &str) -> Option<&str> {
        self.get_path(path)?.as_str()
    }

    /// Get a string array at a dotted path
    pub fn get_string_array(&self, path: &str) -> Option<Vec<&str>> {
        let arr = self.get_path(path)?.as_array()?;
        arr.iter().map(|v| v.as_str()).collect()
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::String(s) => write!(f, "\"{s}\""),
            Value::Integer(n) => write!(f, "{n}"),
            Value::Float(n) => write!(f, "{n}"),
            Value::Boolean(b) => write!(f, "{b}"),
            Value::Array(arr) => {
                write!(f, "[")?;
                for (i, v) in arr.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
            Value::Table(table) => {
                write!(f, "{{")?;
                for (i, (k, v)) in table.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k} = {v}")?;
                }
                write!(f, "}}")
            }
        }
    }
}
