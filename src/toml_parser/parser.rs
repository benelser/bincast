use std::collections::BTreeMap;

use super::lexer::{Lexer, Token};
use super::value::Value;
use crate::error::{TomlError, Error};

/// Parse a TOML string into a Value::Table.
pub fn parse(input: &str) -> Result<Value, Error> {
    let mut lexer = Lexer::new(input);
    let tokens = lexer.tokenize().map_err(|msg| TomlError {
        message: msg,
        line: lexer.line,
        col: lexer.col,
    })?;
    let mut parser = Parser::new(&tokens);
    parser.parse_document().map_err(|msg| {
        Error::TomlParse(TomlError {
            message: msg,
            line: 0,
            col: parser.pos,
        })
    })
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Parser { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&Token> {
        let tok = self.tokens.get(self.pos)?;
        self.pos += 1;
        Some(tok)
    }

    fn expect(&mut self, expected: &Token) -> Result<(), String> {
        match self.advance() {
            Some(tok) if tok == expected => Ok(()),
            Some(tok) => Err(format!("expected {expected:?}, got {tok:?}")),
            None => Err(format!("expected {expected:?}, got EOF")),
        }
    }

    fn skip_newlines(&mut self) {
        while self.peek() == Some(&Token::Newline) {
            self.advance();
        }
    }

    fn read_key(&mut self) -> Result<String, String> {
        match self.advance() {
            Some(Token::Key(k)) => Ok(k.clone()),
            Some(Token::StringVal(s)) => Ok(s.clone()),
            Some(Token::Integer(n)) => Ok(n.to_string()),
            Some(tok) => Err(format!("expected key, got {tok:?}")),
            None => Err("expected key, got EOF".into()),
        }
    }

    /// Read a dotted key like `package.metadata.deb`
    fn read_dotted_key(&mut self) -> Result<Vec<String>, String> {
        let mut keys = vec![self.read_key()?];
        while self.peek() == Some(&Token::Dot) {
            self.advance(); // consume dot
            keys.push(self.read_key()?);
        }
        Ok(keys)
    }

    fn read_value(&mut self) -> Result<Value, String> {
        match self.peek() {
            Some(Token::StringVal(_)) => {
                if let Some(Token::StringVal(s)) = self.advance() {
                    Ok(Value::String(s.clone()))
                } else {
                    unreachable!()
                }
            }
            Some(Token::Integer(_)) => {
                if let Some(Token::Integer(n)) = self.advance() {
                    Ok(Value::Integer(*n))
                } else {
                    unreachable!()
                }
            }
            Some(Token::Float(_)) => {
                if let Some(Token::Float(n)) = self.advance() {
                    Ok(Value::Float(*n))
                } else {
                    unreachable!()
                }
            }
            Some(Token::Boolean(_)) => {
                if let Some(Token::Boolean(b)) = self.advance() {
                    Ok(Value::Boolean(*b))
                } else {
                    unreachable!()
                }
            }
            Some(Token::LeftBracket) => self.read_array(),
            Some(Token::LeftBrace) => self.read_inline_table(),
            Some(tok) => Err(format!("expected value, got {tok:?}")),
            None => Err("expected value, got EOF".into()),
        }
    }

    fn read_array(&mut self) -> Result<Value, String> {
        self.advance(); // consume [
        let mut arr = Vec::new();
        self.skip_newlines();
        while self.peek() != Some(&Token::RightBracket) {
            let val = self.read_value()?;
            arr.push(val);
            self.skip_newlines();
            if self.peek() == Some(&Token::Comma) {
                self.advance();
                self.skip_newlines();
            }
        }
        self.expect(&Token::RightBracket)?;
        Ok(Value::Array(arr))
    }

    fn read_inline_table(&mut self) -> Result<Value, String> {
        self.advance(); // consume {
        let mut table = BTreeMap::new();
        while self.peek() != Some(&Token::RightBrace) {
            let key = self.read_key()?;
            self.expect(&Token::Equals)?;
            let val = self.read_value()?;
            table.insert(key, val);
            if self.peek() == Some(&Token::Comma) {
                self.advance();
            }
        }
        self.expect(&Token::RightBrace)?;
        Ok(Value::Table(table))
    }

    /// Insert a value at a dotted key path into a table, creating intermediate tables as needed.
    fn insert_at_path(
        table: &mut BTreeMap<String, Value>,
        keys: &[String],
        value: Value,
    ) -> Result<(), String> {
        if keys.len() == 1 {
            table.insert(keys[0].clone(), value);
            return Ok(());
        }
        let entry = table
            .entry(keys[0].clone())
            .or_insert_with(|| Value::Table(BTreeMap::new()));
        match entry {
            Value::Table(sub) => Self::insert_at_path(sub, &keys[1..], value),
            _ => Err(format!("key '{}' is not a table", keys[0])),
        }
    }

    /// Get or create a mutable reference to a nested table.
    fn get_or_create_table<'t>(
        root: &'t mut BTreeMap<String, Value>,
        keys: &[String],
    ) -> Result<&'t mut BTreeMap<String, Value>, String> {
        let mut current = root;
        for key in keys {
            let entry = current
                .entry(key.clone())
                .or_insert_with(|| Value::Table(BTreeMap::new()));
            match entry {
                Value::Table(sub) => current = sub,
                _ => return Err(format!("key '{key}' is not a table")),
            }
        }
        Ok(current)
    }

    fn parse_document(&mut self) -> Result<Value, String> {
        let mut root = BTreeMap::new();
        let mut current_table: Vec<String> = Vec::new();

        self.skip_newlines();

        while self.pos < self.tokens.len() {
            match self.peek() {
                Some(Token::Newline) => {
                    self.advance();
                }
                Some(Token::LeftBracket) => {
                    self.advance();
                    // Check for array-of-tables [[...]]
                    if self.peek() == Some(&Token::LeftBracket) {
                        self.advance();
                        let keys = self.read_dotted_key()?;
                        self.expect(&Token::RightBracket)?;
                        self.expect(&Token::RightBracket)?;

                        // Array of tables: append a new table to the array
                        let parent_keys = &keys[..keys.len() - 1];
                        let last_key = &keys[keys.len() - 1];
                        let parent = Self::get_or_create_table(&mut root, parent_keys)?;
                        let entry = parent
                            .entry(last_key.clone())
                            .or_insert_with(|| Value::Array(Vec::new()));
                        match entry {
                            Value::Array(arr) => {
                                arr.push(Value::Table(BTreeMap::new()));
                            }
                            _ => return Err(format!("key '{last_key}' is not an array of tables")),
                        }
                        current_table = keys;
                    } else {
                        let keys = self.read_dotted_key()?;
                        self.expect(&Token::RightBracket)?;
                        // Ensure the table exists
                        Self::get_or_create_table(&mut root, &keys)?;
                        current_table = keys;
                    }
                }
                Some(Token::Key(_) | Token::StringVal(_) | Token::Integer(_)) => {
                    let keys = self.read_dotted_key()?;
                    self.expect(&Token::Equals)?;
                    let val = self.read_value()?;

                    if current_table.is_empty() {
                        Self::insert_at_path(&mut root, &keys, val)?;
                    } else {
                        // Check if we're in an array-of-tables
                        let last_table_key = &current_table[current_table.len() - 1];
                        let parent_keys = &current_table[..current_table.len() - 1];
                        let parent = Self::get_or_create_table(&mut root, parent_keys)?;

                        if let Some(Value::Array(arr)) = parent.get_mut(last_table_key) {
                            // Insert into the last element of the array
                            if let Some(Value::Table(last)) = arr.last_mut() {
                                Self::insert_at_path(last, &keys, val)?;
                            }
                        } else {
                            let table = Self::get_or_create_table(&mut root, &current_table)?;
                            Self::insert_at_path(table, &keys, val)?;
                        }
                    }
                }
                Some(tok) => {
                    return Err(format!("unexpected token: {tok:?}"));
                }
                None => break,
            }
        }

        Ok(Value::Table(root))
    }
}
