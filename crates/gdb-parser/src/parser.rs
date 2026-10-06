use crate::ast::*;
use gdb_core::{DataValue, DataType, Direction, GdbError, GdbResult, PropertySpec, VertexId};

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Call,
    Yield,
    Create,
    Vertex,
    Edge,
    Insert,
    Delete,
    From,
    To,
    Rank,
    Values,
    Match,
    Where,
    Return,
    OrderBy,
    Asc,
    Desc,
    Limit,
    Count,
    And,
    Or,
    Ident(String),
    StringLit(String),
    IntLit(i64),
    FloatLit(f64),
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Colon,
    Semicolon,
    Comma,
    Dot,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Dash,
    RArrow, // ->
    Star,
    Plus,
}

pub struct Lexer {
    
    chars: Vec<(usize, char)>,
    pos: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self {
            
            chars: input.char_indices().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).map(|(_, c)| *c)
    }

    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.pos + 1).map(|(_, c)| *c)
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    pub fn tokenize(&mut self) -> GdbResult<Vec<Token>> {
        let mut tokens = Vec::new();
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.advance();
                continue;
            }

            match ch {
                '(' => { self.advance(); tokens.push(Token::LParen); }
                ')' => { self.advance(); tokens.push(Token::RParen); }
                '[' => { self.advance(); tokens.push(Token::LBracket); }
                ']' => { self.advance(); tokens.push(Token::RBracket); }
                '{' => { self.advance(); tokens.push(Token::LBrace); }
                '}' => { self.advance(); tokens.push(Token::RBrace); }
                ':' => { self.advance(); tokens.push(Token::Colon); }
                ';' => { self.advance(); tokens.push(Token::Semicolon); }
                ',' => { self.advance(); tokens.push(Token::Comma); }
                '.' => { self.advance(); tokens.push(Token::Dot); }
                '*' => { self.advance(); tokens.push(Token::Star); }
                '+' => { self.advance(); tokens.push(Token::Plus); }
                '=' => { self.advance(); tokens.push(Token::Eq); }
                '!' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        tokens.push(Token::NotEq);
                    } else {
                        return Err(GdbError::Parser("Unexpected character '!'".into()));
                    }
                }
                '<' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        tokens.push(Token::LtEq);
                    } else if self.peek() == Some('>') {
                        self.advance();
                        tokens.push(Token::NotEq);
                    } else {
                        tokens.push(Token::Lt);
                    }
                }
                '>' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        tokens.push(Token::GtEq);
                    } else {
                        tokens.push(Token::Gt);
                    }
                }
                '-' => {
                    self.advance();
                    if self.peek() == Some('>') {
                        self.advance();
                        tokens.push(Token::RArrow);
                    } else {
                        tokens.push(Token::Dash);
                    }
                }
                '\'' | '"' => {
                    let quote = ch;
                    self.advance();
                    let mut s = String::new();
                    while let Some(c) = self.peek() {
                        if c == quote {
                            self.advance();
                            break;
                        }
                        s.push(c);
                        self.advance();
                    }
                    tokens.push(Token::StringLit(s));
                }
                '0'..='9' => {
                    let mut num_str = String::new();
                    let mut is_float = false;
                    while let Some(c) = self.peek() {
                        if c.is_ascii_digit() {
                            num_str.push(c);
                            self.advance();
                        } else if c == '.' && !is_float && self.peek_next().map(|p| p.is_ascii_digit()).unwrap_or(false) {
                            is_float = true;
                            num_str.push(c);
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    if is_float {
                        let f: f64 = num_str.parse().map_err(|e| GdbError::Parser(format!("Invalid float: {}", e)))?;
                        tokens.push(Token::FloatLit(f));
                    } else {
                        let i: i64 = num_str.parse().map_err(|e| GdbError::Parser(format!("Invalid int: {}", e)))?;
                        tokens.push(Token::IntLit(i));
                    }
                }
                _ if ch.is_alphabetic() || ch == '_' => {
                    let mut ident = String::new();
                    while let Some(c) = self.peek() {
                        if c.is_alphanumeric() || c == '_' {
                            ident.push(c);
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    let upper = ident.to_uppercase();
                    let token = match upper.as_str() {
                        "CALL" => Token::Call,
                        "YIELD" => Token::Yield,
                        "CREATE" => Token::Create,
                        "VERTEX" => Token::Vertex,
                        "EDGE" => Token::Edge,
                        "INSERT" => Token::Insert,
                        "DELETE" => Token::Delete,
                        "FROM" => Token::From,
                        "TO" => Token::To,
                        "RANK" => Token::Rank,
                        "VALUES" => Token::Values,
                        "MATCH" => Token::Match,
                        "WHERE" => Token::Where,
                        "RETURN" => Token::Return,
                        "ORDER" => Token::OrderBy,
                        "ASC" => Token::Asc,
                        "DESC" => Token::Desc,
                        "LIMIT" => Token::Limit,
                        "COUNT" => Token::Count,
                        "AND" => Token::And,
                        "OR" => Token::Or,
                        _ => Token::Ident(ident),
                    };
                    tokens.push(token);
                }
                other => return Err(GdbError::Parser(format!("Unexpected character: {}", other))),
            }
        }
        Ok(tokens)
    }
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&Token> {
        let tok = self.tokens.get(self.pos);
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn expect(&mut self, expected: &Token) -> GdbResult<()> {
        match self.peek() {
            Some(t) if t == expected => {
                self.advance();
                Ok(())
            }
            Some(other) => Err(GdbError::Parser(format!("Expected {:?}, found {:?}", expected, other))),
            None => Err(GdbError::Parser(format!("Expected {:?}, found EOF", expected))),
        }
    }

    fn expect_ident(&mut self) -> GdbResult<String> {
        match self.peek() {
            Some(Token::Ident(s)) => {
                let res = s.clone();
                self.advance();
                Ok(res)
            }
            Some(other) => Err(GdbError::Parser(format!("Expected identifier, found {:?}", other))),
            None => Err(GdbError::Parser("Expected identifier, found EOF".into())),
        }
    }

    pub fn parse_statement(&mut self) -> GdbResult<Statement> {
        if self.peek() == Some(&Token::Semicolon) {
            self.advance();
        }
        let stmt = match self.peek() {
            Some(Token::Call) => self.parse_call(),
            Some(Token::Create) => self.parse_create(),
            Some(Token::Insert) => self.parse_insert(),
            Some(Token::Delete) => self.parse_delete(),
            Some(Token::Match) => self.parse_query(),
            Some(other) => Err(GdbError::Parser(format!("Unexpected leading token: {:?}", other))),
            None => Err(GdbError::Parser("Empty query".into())),
        }?;

        if self.peek() == Some(&Token::Semicolon) {
            self.advance();
        }
        Ok(stmt)
    }

    fn parse_call(&mut self) -> GdbResult<Statement> {
        self.expect(&Token::Call)?;
        let mut algo_name = self.expect_ident()?;
        if self.peek() == Some(&Token::Dot) {
            self.advance();
            let sub = self.expect_ident()?;
            algo_name = format!("{}.{}", algo_name, sub);
        }

        let mut args = std::collections::HashMap::new();
        if self.peek() == Some(&Token::LParen) {
            self.advance();
            // Check if arguments in JSON-like braces: { key: val, ... }
            if self.peek() == Some(&Token::LBrace) {
                self.advance();
                while self.peek() != Some(&Token::RBrace) {
                    let key = self.expect_ident()?;
                    self.expect(&Token::Colon)?;
                    let val = self.parse_literal()?;
                    args.insert(key, val);
                    if self.peek() == Some(&Token::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                self.expect(&Token::RBrace)?;
            } else {
                // Key-value or empty
                while self.peek() != Some(&Token::RParen) {
                    let key = self.expect_ident()?;
                    self.expect(&Token::Colon)?;
                    let val = self.parse_literal()?;
                    args.insert(key, val);
                    if self.peek() == Some(&Token::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
            }
            self.expect(&Token::RParen)?;
        }

        let mut yield_items = Vec::new();
        if self.peek() == Some(&Token::Yield) {
            self.advance();
            loop {
                let col = self.expect_ident()?;
                yield_items.push(col);
                if self.peek() == Some(&Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }

        Ok(Statement::CallAlgorithm {
            algorithm: algo_name,
            args,
            yield_items,
        })
    }

    fn parse_create(&mut self) -> GdbResult<Statement> {
        self.expect(&Token::Create)?;
        match self.peek() {
            Some(Token::Vertex) => {
                self.advance();
                let label = self.expect_ident()?;
                self.expect(&Token::LParen)?;
                let properties = self.parse_property_specs()?;
                self.expect(&Token::RParen)?;
                Ok(Statement::CreateVertexLabel { label, properties })
            }
            Some(Token::Edge) => {
                self.advance();
                let edge_type = self.expect_ident()?;
                let properties = if self.peek() == Some(&Token::LParen) {
                    self.advance();
                    let props = self.parse_property_specs()?;
                    self.expect(&Token::RParen)?;
                    props
                } else {
                    Vec::new()
                };
                Ok(Statement::CreateEdgeType { edge_type, properties })
            }
            Some(other) => Err(GdbError::Parser(format!("Expected VERTEX or EDGE after CREATE, found {:?}", other))),
            None => Err(GdbError::Parser("Unexpected EOF after CREATE".into())),
        }
    }

    fn parse_property_specs(&mut self) -> GdbResult<Vec<PropertySpec>> {
        let mut specs = Vec::new();
        if self.peek() == Some(&Token::RParen) {
            return Ok(specs);
        }

        loop {
            let name = self.expect_ident()?;
            let type_ident = self.expect_ident()?.to_uppercase();
            let data_type = match type_ident.as_str() {
                "STRING" | "VARCHAR" | "TEXT" => DataType::String,
                "INT" | "INT64" | "BIGINT" | "INTEGER" => DataType::Int64,
                "FLOAT" | "FLOAT64" | "DOUBLE" => DataType::Float64,
                "BOOL" | "BOOLEAN" => DataType::Boolean,
                "DATE" => DataType::Date,
                "TIMESTAMP" => DataType::Timestamp,
                other => return Err(GdbError::Parser(format!("Unknown data type: {}", other))),
            };
            specs.push(PropertySpec::new(name, data_type, true));

            if self.peek() == Some(&Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        Ok(specs)
    }

    fn parse_insert(&mut self) -> GdbResult<Statement> {
        self.expect(&Token::Insert)?;
        match self.peek() {
            Some(Token::Vertex) => {
                self.advance();
                let label = self.expect_ident()?;
                self.expect(&Token::LParen)?;
                let mut prop_names = Vec::new();
                while self.peek() != Some(&Token::RParen) {
                    prop_names.push(self.expect_ident()?);
                    if self.peek() == Some(&Token::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                self.expect(&Token::RParen)?;
                self.expect(&Token::Values)?;
                self.expect(&Token::LParen)?;

                let mut values = Vec::new();
                while self.peek() != Some(&Token::RParen) {
                    values.push(self.parse_literal()?);
                    if self.peek() == Some(&Token::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                self.expect(&Token::RParen)?;

                if prop_names.len() != values.len() {
                    return Err(GdbError::Parser("Property names and values count mismatch in INSERT VERTEX".into()));
                }

                let mut vid: Option<VertexId> = None;
                let mut props = Vec::new();

                for (name, val) in prop_names.into_iter().zip(values.into_iter()) {
                    if name.eq_ignore_ascii_case("id") || name.eq_ignore_ascii_case("_id") {
                        match val {
                            DataValue::Int64(i) => vid = Some(VertexId(i as u64)),
                            DataValue::String(s) => vid = Some(VertexId::from_str_key(&s)),
                            _ => return Err(GdbError::Parser("Vertex id must be integer or string".into())),
                        }
                    } else {
                        props.push((name, val));
                    }
                }

                let id = vid.ok_or_else(|| GdbError::Parser("INSERT VERTEX requires an 'id' column".into()))?;
                Ok(Statement::InsertVertex { label, id, properties: props })
            }
            Some(Token::Edge) => {
                self.advance();
                let edge_type = self.expect_ident()?;
                self.expect(&Token::From)?;
                let src = self.parse_vertex_id()?;
                self.expect(&Token::To)?;
                let dst = self.parse_vertex_id()?;

                let mut rank = 0i64;
                if self.peek() == Some(&Token::Rank) {
                    self.advance();
                    match self.advance() {
                        Some(Token::IntLit(r)) => rank = *r,
                        _ => return Err(GdbError::Parser("Expected integer after RANK".into())),
                    }
                }

                Ok(Statement::InsertEdge {
                    edge_type,
                    src,
                    dst,
                    rank,
                    properties: Vec::new(),
                })
            }
            Some(other) => Err(GdbError::Parser(format!("Expected VERTEX or EDGE after INSERT, found {:?}", other))),
            None => Err(GdbError::Parser("Unexpected EOF after INSERT".into())),
        }
    }

    fn parse_delete(&mut self) -> GdbResult<Statement> {
        self.expect(&Token::Delete)?;
        self.expect(&Token::Edge)?;
        let edge_type = self.expect_ident()?;
        self.expect(&Token::From)?;
        let src = self.parse_vertex_id()?;
        self.expect(&Token::To)?;
        let dst = self.parse_vertex_id()?;
        Ok(Statement::DeleteEdge { edge_type, src, dst, rank: 0 })
    }

    fn parse_vertex_id(&mut self) -> GdbResult<VertexId> {
        match self.advance() {
            Some(Token::IntLit(i)) => Ok(VertexId(*i as u64)),
            Some(Token::StringLit(s)) => Ok(VertexId::from_str_key(s)),
            Some(other) => Err(GdbError::Parser(format!("Expected vertex ID literal, found {:?}", other))),
            None => Err(GdbError::Parser("Expected vertex ID literal, found EOF".into())),
        }
    }

    fn parse_literal(&mut self) -> GdbResult<DataValue> {
        match self.advance() {
            Some(Token::IntLit(i)) => Ok(DataValue::Int64(*i)),
            Some(Token::FloatLit(f)) => Ok(DataValue::Float64(*f)),
            Some(Token::StringLit(s)) => Ok(DataValue::String(s.clone())),
            Some(Token::Ident(s)) if s.eq_ignore_ascii_case("true") => Ok(DataValue::Boolean(true)),
            Some(Token::Ident(s)) if s.eq_ignore_ascii_case("false") => Ok(DataValue::Boolean(false)),
            Some(Token::Ident(s)) if s.eq_ignore_ascii_case("null") => Ok(DataValue::Null),
            Some(other) => Err(GdbError::Parser(format!("Expected literal, found {:?}", other))),
            None => Err(GdbError::Parser("Expected literal, found EOF".into())),
        }
    }

    pub fn parse_query(&mut self) -> GdbResult<Statement> {
        self.expect(&Token::Match)?;
        let pattern = self.parse_path_pattern()?;

        let mut where_clause = None;
        if self.peek() == Some(&Token::Where) {
            self.advance();
            where_clause = Some(self.parse_expr()?);
        }

        self.expect(&Token::Return)?;
        let mut return_items = Vec::new();
        loop {
            let expr = self.parse_expr()?;
            let mut alias = None;
            if let Some(Token::Ident(a)) = self.peek() {
                if !a.eq_ignore_ascii_case("LIMIT") && !a.eq_ignore_ascii_case("ORDER") {
                    alias = Some(a.clone());
                    self.advance();
                }
            }
            return_items.push(ReturnItem { expr, alias });
            if self.peek() == Some(&Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        let mut limit = None;
        if self.peek() == Some(&Token::Limit) {
            self.advance();
            match self.advance() {
                Some(Token::IntLit(l)) => limit = Some(*l as usize),
                _ => return Err(GdbError::Parser("Expected integer after LIMIT".into())),
            }
        }

        Ok(Statement::Query(CypherQuery {
            pattern,
            where_clause,
            return_items,
            order_by: Vec::new(),
            limit,
        }))
    }

    fn parse_path_pattern(&mut self) -> GdbResult<PathPattern> {
        let start_node = self.parse_node_pattern()?;
        let mut hops = Vec::new();

        while self.peek() == Some(&Token::Dash) {
            let edge = self.parse_edge_pattern()?;
            let target = self.parse_node_pattern()?;
            hops.push((edge, target));
        }

        Ok(PathPattern { start_node, hops })
    }

    fn parse_node_pattern(&mut self) -> GdbResult<NodePattern> {
        self.expect(&Token::LParen)?;
        let mut variable = None;
        let mut label = None;

        if let Some(Token::Ident(v)) = self.peek() {
            variable = Some(v.clone());
            self.advance();
        }

        if self.peek() == Some(&Token::Colon) {
            self.advance();
            label = Some(self.expect_ident()?);
        }

        self.expect(&Token::RParen)?;
        Ok(NodePattern { variable, label, id_filter: None })
    }

    fn parse_edge_pattern(&mut self) -> GdbResult<EdgePattern> {
        self.expect(&Token::Dash)?;
        let mut variable = None;
        let mut edge_type = None;

        if self.peek() == Some(&Token::LBracket) {
            self.advance();
            if let Some(Token::Ident(v)) = self.peek() {
                variable = Some(v.clone());
                self.advance();
            }
            if self.peek() == Some(&Token::Colon) {
                self.advance();
                edge_type = Some(self.expect_ident()?);
            }
            self.expect(&Token::RBracket)?;
        }

        self.expect(&Token::RArrow)?;
        Ok(EdgePattern {
            variable,
            edge_type,
            direction: Direction::Out,
        })
    }

    fn parse_expr(&mut self) -> GdbResult<Expr> {
        self.parse_or_expr()
    }

    fn parse_or_expr(&mut self) -> GdbResult<Expr> {
        let mut left = self.parse_and_expr()?;
        while self.peek() == Some(&Token::Or) {
            self.advance();
            let right = self.parse_and_expr()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::Or,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and_expr(&mut self) -> GdbResult<Expr> {
        let mut left = self.parse_comparison_expr()?;
        while self.peek() == Some(&Token::And) {
            self.advance();
            let right = self.parse_comparison_expr()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::And,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_comparison_expr(&mut self) -> GdbResult<Expr> {
        let left = self.parse_primary_expr()?;
        let op = match self.peek() {
            Some(Token::Eq) => Some(BinaryOperator::Eq),
            Some(Token::NotEq) => Some(BinaryOperator::NotEq),
            Some(Token::Lt) => Some(BinaryOperator::Lt),
            Some(Token::LtEq) => Some(BinaryOperator::LtEq),
            Some(Token::Gt) => Some(BinaryOperator::Gt),
            Some(Token::GtEq) => Some(BinaryOperator::GtEq),
            _ => None,
        };

        if let Some(bin_op) = op {
            self.advance();
            let right = self.parse_primary_expr()?;
            Ok(Expr::BinaryOp {
                left: Box::new(left),
                op: bin_op,
                right: Box::new(right),
            })
        } else {
            Ok(left)
        }
    }

    fn parse_primary_expr(&mut self) -> GdbResult<Expr> {
        match self.peek() {
            Some(Token::Count) => {
                self.advance();
                self.expect(&Token::LParen)?;
                self.expect(&Token::Star)?;
                self.expect(&Token::RParen)?;
                Ok(Expr::CountStar)
            }
            Some(Token::IntLit(i)) => {
                let v = *i;
                self.advance();
                Ok(Expr::Literal(DataValue::Int64(v)))
            }
            Some(Token::FloatLit(f)) => {
                let v = *f;
                self.advance();
                Ok(Expr::Literal(DataValue::Float64(v)))
            }
            Some(Token::StringLit(s)) => {
                let v = s.clone();
                self.advance();
                Ok(Expr::Literal(DataValue::String(v)))
            }
            Some(Token::Ident(var)) => {
                let var_name = var.clone();
                self.advance();
                if self.peek() == Some(&Token::Dot) {
                    self.advance();
                    let prop_name = self.expect_ident()?;
                    Ok(Expr::Property { variable: var_name, property: prop_name })
                } else {
                    Ok(Expr::Variable(var_name))
                }
            }
            Some(other) => Err(GdbError::Parser(format!("Unexpected token in expression: {:?}", other))),
            None => Err(GdbError::Parser("Unexpected EOF in expression".into())),
        }
    }
}

pub fn parse(input: &str) -> GdbResult<Statement> {
    let mut lexer = Lexer::new(input);
    let tokens = lexer.tokenize()?;
    let mut parser = Parser::new(tokens);
    parser.parse_statement()
}
