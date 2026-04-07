/// TOML lexer — tokenizes input for the parser.
/// Supports: bare keys, quoted keys, strings (basic + literal),
/// integers, floats, booleans, arrays, inline tables, tables, array-of-tables.

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// A bare key or quoted key
    Key(String),
    /// A basic or literal string value
    StringVal(String),
    /// An integer value
    Integer(i64),
    /// A float value
    Float(f64),
    /// true or false
    Boolean(bool),
    /// =
    Equals,
    /// [
    LeftBracket,
    /// ]
    RightBracket,
    /// {
    LeftBrace,
    /// }
    RightBrace,
    /// ,
    Comma,
    /// .
    Dot,
    /// End of line (newline or EOF)
    Newline,
}

pub struct Lexer<'a> {
    input: &'a str,
    bytes: &'a [u8],
    pos: usize,
    pub line: usize,
    pub col: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Lexer {
            input,
            bytes: input.as_bytes(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<u8> {
        let b = self.bytes.get(self.pos).copied()?;
        self.pos += 1;
        if b == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(b)
    }

    fn skip_whitespace(&mut self) {
        while let Some(b) = self.peek() {
            if b == b' ' || b == b'\t' || b == b'\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_comment(&mut self) {
        if self.peek() == Some(b'#') {
            while let Some(b) = self.peek() {
                if b == b'\n' {
                    break;
                }
                self.advance();
            }
        }
    }

    fn read_basic_string(&mut self) -> Result<String, String> {
        self.advance(); // consume opening "
        // Check for multiline """
        if self.peek() == Some(b'"') {
            let saved_pos = self.pos;
            self.advance();
            if self.peek() == Some(b'"') {
                self.advance();
                return self.read_multiline_basic_string();
            }
            // It was just an empty string ""
            self.pos = saved_pos;
            self.advance(); // consume closing "
            return Ok(String::new());
        }
        let mut s = String::new();
        loop {
            match self.advance() {
                None => return Err("unterminated string".into()),
                Some(b'"') => return Ok(s),
                Some(b'\\') => {
                    match self.advance() {
                        Some(b'"') => s.push('"'),
                        Some(b'\\') => s.push('\\'),
                        Some(b'n') => s.push('\n'),
                        Some(b't') => s.push('\t'),
                        Some(b'r') => s.push('\r'),
                        Some(c) => return Err(format!("unknown escape: \\{}", c as char)),
                        None => return Err("unterminated escape".into()),
                    }
                }
                Some(b) => s.push(b as char),
            }
        }
    }

    fn read_multiline_basic_string(&mut self) -> Result<String, String> {
        // Skip first newline if immediately after """
        if self.peek() == Some(b'\n') {
            self.advance();
        } else if self.peek() == Some(b'\r') {
            self.advance();
            if self.peek() == Some(b'\n') {
                self.advance();
            }
        }
        let mut s = String::new();
        loop {
            match self.advance() {
                None => return Err("unterminated multiline string".into()),
                Some(b'"') => {
                    if self.peek() == Some(b'"') {
                        self.advance();
                        if self.peek() == Some(b'"') {
                            self.advance();
                            return Ok(s);
                        }
                        s.push('"');
                        s.push('"');
                    } else {
                        s.push('"');
                    }
                }
                Some(b'\\') => {
                    match self.advance() {
                        Some(b'"') => s.push('"'),
                        Some(b'\\') => s.push('\\'),
                        Some(b'n') => s.push('\n'),
                        Some(b't') => s.push('\t'),
                        Some(b'r') => s.push('\r'),
                        Some(b'\n') => {
                            // line-ending backslash: skip whitespace
                            while let Some(b) = self.peek() {
                                if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
                                    self.advance();
                                } else {
                                    break;
                                }
                            }
                        }
                        Some(c) => return Err(format!("unknown escape: \\{}", c as char)),
                        None => return Err("unterminated escape in multiline string".into()),
                    }
                }
                Some(b) => s.push(b as char),
            }
        }
    }

    fn read_literal_string(&mut self) -> Result<String, String> {
        self.advance(); // consume opening '
        // Check for multiline '''
        if self.peek() == Some(b'\'') {
            let saved_pos = self.pos;
            self.advance();
            if self.peek() == Some(b'\'') {
                self.advance();
                return self.read_multiline_literal_string();
            }
            self.pos = saved_pos;
            self.advance();
            return Ok(String::new());
        }
        let mut s = String::new();
        loop {
            match self.advance() {
                None => return Err("unterminated literal string".into()),
                Some(b'\'') => return Ok(s),
                Some(b) => s.push(b as char),
            }
        }
    }

    fn read_multiline_literal_string(&mut self) -> Result<String, String> {
        if self.peek() == Some(b'\n') {
            self.advance();
        } else if self.peek() == Some(b'\r') {
            self.advance();
            if self.peek() == Some(b'\n') {
                self.advance();
            }
        }
        let mut s = String::new();
        loop {
            match self.advance() {
                None => return Err("unterminated multiline literal string".into()),
                Some(b'\'') => {
                    if self.peek() == Some(b'\'') {
                        self.advance();
                        if self.peek() == Some(b'\'') {
                            self.advance();
                            return Ok(s);
                        }
                        s.push('\'');
                        s.push('\'');
                    } else {
                        s.push('\'');
                    }
                }
                Some(b) => s.push(b as char),
            }
        }
    }

    fn read_bare_key_or_value(&mut self) -> Token {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' {
                self.advance();
            } else {
                break;
            }
        }
        let word = &self.input[start..self.pos];
        match word {
            "true" => Token::Boolean(true),
            "false" => Token::Boolean(false),
            _ => {
                // Try parsing as integer
                if let Ok(n) = word.parse::<i64>() {
                    Token::Integer(n)
                } else if let Ok(n) = word.parse::<f64>() {
                    Token::Float(n)
                } else {
                    Token::Key(word.to_string())
                }
            }
        }
    }

    fn read_number(&mut self) -> Token {
        let start = self.pos;
        let negative = self.peek() == Some(b'-') || self.peek() == Some(b'+');
        if negative {
            self.advance();
        }
        let mut is_float = false;
        while let Some(b) = self.peek() {
            if b.is_ascii_digit() || b == b'_' {
                self.advance();
            } else if b == b'.' || b == b'e' || b == b'E' {
                is_float = true;
                self.advance();
            } else if b == b'+' || b == b'-' {
                // Only valid after e/E
                self.advance();
            } else {
                break;
            }
        }
        let word: String = self.input[start..self.pos].chars().filter(|c| *c != '_').collect();
        if is_float {
            Token::Float(word.parse().unwrap_or(0.0))
        } else {
            Token::Integer(word.parse().unwrap_or(0))
        }
    }

    pub fn next_token(&mut self) -> Result<Option<Token>, String> {
        self.skip_whitespace();
        self.skip_comment();

        match self.peek() {
            None => Ok(None),
            Some(b'\n') => {
                self.advance();
                Ok(Some(Token::Newline))
            }
            Some(b'=') => {
                self.advance();
                Ok(Some(Token::Equals))
            }
            Some(b'[') => {
                self.advance();
                Ok(Some(Token::LeftBracket))
            }
            Some(b']') => {
                self.advance();
                Ok(Some(Token::RightBracket))
            }
            Some(b'{') => {
                self.advance();
                Ok(Some(Token::LeftBrace))
            }
            Some(b'}') => {
                self.advance();
                Ok(Some(Token::RightBrace))
            }
            Some(b',') => {
                self.advance();
                Ok(Some(Token::Comma))
            }
            Some(b'.') => {
                self.advance();
                Ok(Some(Token::Dot))
            }
            Some(b'"') => {
                let s = self.read_basic_string()?;
                Ok(Some(Token::StringVal(s)))
            }
            Some(b'\'') => {
                let s = self.read_literal_string()?;
                Ok(Some(Token::StringVal(s)))
            }
            Some(b) if b == b'-' || b == b'+' || b.is_ascii_digit() => {
                Ok(Some(self.read_number()))
            }
            Some(b) if b.is_ascii_alphabetic() || b == b'_' => {
                Ok(Some(self.read_bare_key_or_value()))
            }
            Some(b) => Err(format!("unexpected character: '{}' at {}:{}", b as char, self.line, self.col)),
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();
        while let Some(tok) = self.next_token()? {
            tokens.push(tok);
        }
        Ok(tokens)
    }
}
