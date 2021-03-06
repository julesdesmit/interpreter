use crate::ast::Node;
use crate::lexer::Lexer;
use crate::tokens::{Token, TokenType};

/// The different types of operator precedence that can be encountered while
/// parsing multi-layered expressions.
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Precedence {
    Lowest,
    Equals,
    LessGreater,
    Sum,
    Product,
    Prefix,
    Call,
}

/// All the possible errors that can be encountered by the parser.
#[derive(Debug)]
pub enum ParserError {
    TokenUnrecognized,
    IdentExpected,
    AssignExpected,
    IntegerParsingFailed,
    BooleanParsingFailed,
    GroupExpressionParsingFailed,
    IncorrectIfStatement,
    IncorrectFunctionDeclaration,
}

/// The parser for our programming language. The parser uses a lexer to create
/// a tokenized version of the input string, which it can then use to construct
/// an AST.
///
/// This is an implementation of a Pratt parser, or a "top down operator precedence"
/// parser. It parses by means of recursive descent, and thus does not need to
/// backtrack.
pub struct Parser<'a> {
    lexer: Lexer<'a>,
    curr_token: Token,
    peek_token: Token,
    pub errors: Vec<ParserError>,
}

impl<'a> Parser<'a> {
    /// Creates a new parser and provides it with a pre-initialized lexer.
    /// NOTE: This function will load in the first two tokens before returning.
    pub fn new(lexer: Lexer<'a>) -> Parser<'a> {
        let mut parser = Self {
            lexer,
            curr_token: Token::default(),
            peek_token: Token::default(),
            errors: vec![],
        };

        parser.next_token();
        parser.next_token();
        parser
    }

    /// Parse a full program. This will basically consume tokens from the lexer
    /// until it is fully exhausted, or until an error is encountered.
    pub fn parse_program(&mut self) -> Node {
        let mut statements = vec![];

        while !self.finished() {
            match self.parse_statement() {
                Ok(stmt) => statements.push(stmt),
                Err(e) => self.errors.push(e),
            };

            self.next_token();
        }

        Node::Program { statements }
    }

    fn finished(&self) -> bool {
        self.curr_token.t == TokenType::EOF
    }

    fn parse_statement(&mut self) -> Result<Node, ParserError> {
        match self.curr_token.t {
            TokenType::Let => self.parse_let_statement(),
            TokenType::Return => self.parse_return_statement(),
            _ => self.parse_expression_statement(),
        }
    }

    fn parse_let_statement(&mut self) -> Result<Node, ParserError> {
        if !self.expect_peek(TokenType::Ident) {
            return Err(ParserError::IdentExpected);
        }

        let ident = Node::Identifier {
            value: self.curr_token.clone(),
        };

        if !self.expect_peek(TokenType::Assign) {
            return Err(ParserError::AssignExpected);
        }

        self.next_token();
        let value = self.parse_expression(Precedence::Lowest)?;

        if self.peek_token.t == TokenType::Semicolon {
            self.next_token();
        }

        Ok(Node::LetStatement {
            name: Box::new(ident),
            value: Some(Box::new(value)),
        })
    }

    fn parse_return_statement(&mut self) -> Result<Node, ParserError> {
        self.next_token();
        let value = self.parse_expression(Precedence::Lowest)?;
        if self.peek_token.t == TokenType::Semicolon {
            self.next_token();
        }

        Ok(Node::ReturnStatement {
            value: Some(Box::new(value)),
        })
    }

    fn parse_expression_statement(&mut self) -> Result<Node, ParserError> {
        let expr = Node::ExpressionStatement {
            expression: Some(Box::new(self.parse_expression(Precedence::Lowest)?)),
        };

        if self.peek_token.t == TokenType::Semicolon {
            self.next_token();
        }
        Ok(expr)
    }

    fn parse_expression(&mut self, precedence: Precedence) -> Result<Node, ParserError> {
        let mut left_exp = match self.curr_token.t {
            TokenType::Ident => Ok(Node::Identifier {
                value: self.curr_token.clone(),
            }),
            TokenType::Int => self.parse_integer_literal(),
            TokenType::Minus | TokenType::Bang => self.parse_prefix_expression(),
            TokenType::True | TokenType::False => self.parse_boolean_expression(),
            TokenType::LParen => self.parse_grouped_expression(),
            TokenType::If => self.parse_if_expression(),
            TokenType::Function => self.parse_function_literal(),
            _ => Err(ParserError::TokenUnrecognized),
        }?;

        while self.peek_token.t != TokenType::Semicolon && precedence < self.check_peek_precedence()
        {
            if !self.should_keep_parsing() {
                return Ok(left_exp);
            }

            self.next_token();
            left_exp = self.parse_infix_expression(left_exp)?;
        }

        Ok(left_exp)
    }

    fn parse_integer_literal(&mut self) -> Result<Node, ParserError> {
        Ok(Node::IntegerLiteral {
            value: self
                .curr_token
                .v
                .parse()
                .map_err(|_| ParserError::IntegerParsingFailed)?,
        })
    }

    fn parse_prefix_expression(&mut self) -> Result<Node, ParserError> {
        let prefix_token = self.curr_token.clone();

        self.next_token();
        Ok(Node::PrefixExpression {
            operator: prefix_token.v,
            right: Box::new(self.parse_expression(Precedence::Prefix)?),
        })
    }

    fn parse_infix_expression(&mut self, left: Node) -> Result<Node, ParserError> {
        // Special case for call expressions, otherwise we end up overlapping
        // with grouped expressions.
        // TODO: This is obviously poorly designed and needs to be revamped.
        if self.curr_token.t == TokenType::LParen {
            self.next_token();
            return self.parse_call_expression(left);
        }

        let operator = self.curr_token.clone();

        let precedence = self.check_curr_precedence();
        self.next_token();
        Ok(Node::InfixExpression {
            left: Box::new(left),
            operator: operator.v,
            right: Box::new(self.parse_expression(precedence)?),
        })
    }

    fn parse_boolean_expression(&mut self) -> Result<Node, ParserError> {
        Ok(Node::Boolean {
            value: self.curr_token == Token::new(TokenType::True, String::from("true")),
        })
    }

    fn parse_grouped_expression(&mut self) -> Result<Node, ParserError> {
        self.next_token();

        let exp = self.parse_expression(Precedence::Lowest)?;
        if !self.expect_peek(TokenType::RParen) {
            return Err(ParserError::GroupExpressionParsingFailed);
        }

        Ok(exp)
    }

    fn parse_if_expression(&mut self) -> Result<Node, ParserError> {
        self.next_token();
        let condition = self.parse_expression(Precedence::Lowest)?;

        if !self.expect_peek(TokenType::LBrace) {
            return Err(ParserError::IncorrectIfStatement);
        }

        let consequence = self.parse_block_statement()?;

        if self.peek_token.t == TokenType::Else {
            self.next_token();

            if !self.expect_peek(TokenType::LBrace) {
                return Err(ParserError::IncorrectIfStatement);
            }

            let alternative = self.parse_block_statement()?;

            Ok(Node::IfExpression {
                condition: Box::new(condition),
                consequence: Box::new(consequence),
                alternative: Some(Box::new(alternative)),
            })
        } else {
            Ok(Node::IfExpression {
                condition: Box::new(condition),
                consequence: Box::new(consequence),
                alternative: None,
            })
        }
    }

    fn parse_block_statement(&mut self) -> Result<Node, ParserError> {
        let mut statements = vec![];

        self.next_token();
        while self.curr_token.t != TokenType::RBrace && self.curr_token.t != TokenType::EOF {
            let stmt = self.parse_statement()?;
            statements.push(stmt);
            self.next_token();
        }

        Ok(Node::BlockStatement { statements })
    }

    fn parse_function_literal(&mut self) -> Result<Node, ParserError> {
        if !self.expect_peek(TokenType::LParen) {
            return Err(ParserError::IncorrectFunctionDeclaration);
        }

        self.next_token();
        let mut parameters = vec![];
        while self.curr_token.t != TokenType::RParen {
            if self.curr_token.t == TokenType::Comma {
                self.next_token();
            }

            let parameter = Node::Identifier {
                value: self.curr_token.clone(),
            };
            parameters.push(parameter);

            self.next_token();
        }

        self.next_token();
        let body = self.parse_block_statement()?;
        Ok(Node::FunctionLiteral {
            parameters,
            body: Box::new(body),
        })
    }

    fn parse_call_expression(&mut self, function: Node) -> Result<Node, ParserError> {
        let mut arguments = vec![];
        while self.curr_token.t != TokenType::RParen {
            if self.curr_token.t == TokenType::Comma {
                self.next_token();
            }

            let argument = self.parse_expression(Precedence::Lowest)?;
            arguments.push(argument);

            self.next_token();
        }

        Ok(Node::CallExpression {
            function: Box::new(function),
            arguments,
        })
    }

    fn check_curr_precedence(&mut self) -> Precedence {
        match self.curr_token.t {
            TokenType::Equal | TokenType::NotEqual => Precedence::Equals,
            TokenType::LessThan | TokenType::GreaterThan => Precedence::LessGreater,
            TokenType::Plus | TokenType::Minus => Precedence::Sum,
            TokenType::Slash | TokenType::Asterisk => Precedence::Product,
            TokenType::LParen => Precedence::Call,
            _ => Precedence::Lowest,
        }
    }

    fn check_peek_precedence(&mut self) -> Precedence {
        match self.peek_token.t {
            TokenType::Equal | TokenType::NotEqual => Precedence::Equals,
            TokenType::LessThan | TokenType::GreaterThan => Precedence::LessGreater,
            TokenType::Plus | TokenType::Minus => Precedence::Sum,
            TokenType::Slash | TokenType::Asterisk => Precedence::Product,
            TokenType::LParen => Precedence::Call,
            _ => Precedence::Lowest,
        }
    }

    fn should_keep_parsing(&mut self) -> bool {
        matches!(
            self.peek_token.t,
            TokenType::Plus
                | TokenType::Minus
                | TokenType::Slash
                | TokenType::Asterisk
                | TokenType::Equal
                | TokenType::NotEqual
                | TokenType::LessThan
                | TokenType::GreaterThan
                | TokenType::LParen
        )
    }

    fn expect_peek(&mut self, token_type: TokenType) -> bool {
        if self.peek_token.t == token_type {
            self.next_token();
            true
        } else {
            false
        }
    }

    fn next_token(&mut self) {
        self.curr_token = self.peek_token.clone();
        self.peek_token = self.lexer.next_token();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_let_statements() {
        let input = "
            let x = 5;
            let y = 10;
            let z = 838383;";

        let lexer = Lexer::new(input);
        let mut parser = Parser::new(lexer);

        let program = parser.parse_program();
        assert!(!did_parser_fail(parser.errors));

        match program {
            Node::Program { statements } => {
                assert_eq!(3, statements.len());
                let mut iter = statements.iter();

                let first_statement = iter.next().expect("should contain a statement");
                assert_eq!(String::from("let"), first_statement.token_literal());
                if let Node::LetStatement { name, value } = first_statement {
                    assert_eq!(String::from("x"), name.token_literal());
                    assert_eq!(*value, Some(Box::new(Node::IntegerLiteral { value: 5 })));
                } else {
                    panic!("expected let statement");
                }

                let second_statement = iter.next().expect("should contain a statement");
                assert_eq!(String::from("let"), second_statement.token_literal());
                if let Node::LetStatement { name, value } = second_statement {
                    assert_eq!(String::from("y"), name.token_literal());
                    assert_eq!(*value, Some(Box::new(Node::IntegerLiteral { value: 10 })));
                } else {
                    panic!("expected let statement");
                }

                let third_statement = iter.next().expect("should contain a statement");
                assert_eq!(String::from("let"), third_statement.token_literal());
                if let Node::LetStatement { name, value } = third_statement {
                    assert_eq!(String::from("z"), name.token_literal());
                    assert_eq!(
                        *value,
                        Some(Box::new(Node::IntegerLiteral { value: 838383 }))
                    );
                } else {
                    panic!("expected let statement");
                }
            }
            _ => panic!("Unsupported node type"),
        }
    }

    #[test]
    fn test_return_statements() {
        let input = "
            return 5;
            return 10;
            return 987235;";

        let lexer = Lexer::new(input);
        let mut parser = Parser::new(lexer);

        let program = parser.parse_program();
        assert!(!did_parser_fail(parser.errors));

        match program {
            Node::Program { statements } => {
                assert_eq!(3, statements.len());
                let mut iter = statements.iter();

                let first_statement = iter.next().expect("should contain a statement");
                assert_eq!(String::from("return"), first_statement.token_literal());
                if let Node::ReturnStatement { value } = first_statement {
                    assert_eq!(*value, Some(Box::new(Node::IntegerLiteral { value: 5 })));
                } else {
                    panic!("expected return statement");
                }
            }
            _ => panic!("Unexpected node type"),
        }
    }

    #[test]
    fn test_identifier_expression() {
        let input = "foobar;";

        let lexer = Lexer::new(input);
        let mut parser = Parser::new(lexer);

        let program = parser.parse_program();
        assert!(!did_parser_fail(parser.errors));

        match program {
            Node::Program { statements } => {
                assert_eq!(1, statements.len());
                let stmt = statements[0].clone();
                let ident = Node::ExpressionStatement {
                    expression: Some(Box::new(Node::Identifier {
                        value: Token::new(TokenType::Ident, "foobar".to_string()),
                    })),
                };
                assert_eq!(stmt, ident);
                assert_eq!(stmt.token_literal(), "foobar".to_string());
            }
            _ => panic!("Unsupported node type"),
        }
    }

    #[test]
    fn test_integer_literal_expression() {
        let input = "5;";

        let lexer = Lexer::new(input);
        let mut parser = Parser::new(lexer);

        let program = parser.parse_program();
        assert!(!did_parser_fail(parser.errors));

        match program {
            Node::Program { statements } => {
                assert_eq!(1, statements.len());
                let stmt = statements[0].clone();
                let ident = Node::ExpressionStatement {
                    expression: Some(Box::new(Node::IntegerLiteral { value: 5 })),
                };
                assert_eq!(stmt, ident);
                assert_eq!(stmt.token_literal(), "5".to_string());
            }
            _ => panic!("Unsupported node type"),
        }
    }

    #[test]
    fn test_prefix_expression() {
        let input = "
                !5;
                -15;
                !true;
                !false;";

        let lexer = Lexer::new(input);
        let mut parser = Parser::new(lexer);

        let program = parser.parse_program();
        assert!(!did_parser_fail(parser.errors));

        match program {
            Node::Program { statements } => {
                assert_eq!(4, statements.len());
                let mut iter = statements.into_iter();
                let stmt = iter.next().unwrap();
                let ident = Node::ExpressionStatement {
                    expression: Some(Box::new(Node::PrefixExpression {
                        operator: "!".to_string(),
                        right: Box::new(Node::IntegerLiteral { value: 5 }),
                    })),
                };
                assert_eq!(stmt, ident);
                assert_eq!(stmt.token_literal(), "!".to_string());

                let stmt = iter.next().unwrap();
                let ident = Node::ExpressionStatement {
                    expression: Some(Box::new(Node::PrefixExpression {
                        operator: "-".to_string(),
                        right: Box::new(Node::IntegerLiteral { value: 15 }),
                    })),
                };
                assert_eq!(stmt, ident);
                assert_eq!(stmt.token_literal(), "-".to_string());

                let stmt = iter.next().unwrap();
                let ident = Node::ExpressionStatement {
                    expression: Some(Box::new(Node::PrefixExpression {
                        operator: "!".to_string(),
                        right: Box::new(Node::Boolean { value: true }),
                    })),
                };
                assert_eq!(stmt, ident);
                assert_eq!(stmt.token_literal(), "!".to_string());

                let stmt = iter.next().unwrap();
                let ident = Node::ExpressionStatement {
                    expression: Some(Box::new(Node::PrefixExpression {
                        operator: "!".to_string(),
                        right: Box::new(Node::Boolean { value: false }),
                    })),
                };
                assert_eq!(stmt, ident);
                assert_eq!(stmt.token_literal(), "!".to_string());
            }
            _ => panic!("Unsupported node type"),
        }
    }

    #[test]
    fn test_infix_expression() {
        let input = "
                5 + 5;
                5 - 5;
                5 * 5;
                5 / 5;
                5 > 5;
                5 < 5;
                5 == 5;
                5 != 5;
                true == true;
                true != false;
                false == false;";

        let lexer = Lexer::new(input);
        let mut parser = Parser::new(lexer);

        let program = parser.parse_program();
        assert!(!did_parser_fail(parser.errors));

        match program {
            Node::Program { statements } => {
                assert_eq!(11, statements.len());
                let mut iter = statements.into_iter();
                let stmt = iter.next().unwrap();
                let ident = Node::ExpressionStatement {
                    expression: Some(Box::new(Node::InfixExpression {
                        left: Box::new(Node::IntegerLiteral { value: 5 }),
                        operator: "+".to_string(),
                        right: Box::new(Node::IntegerLiteral { value: 5 }),
                    })),
                };
                assert_eq!(stmt, ident);
                assert_eq!(stmt.token_literal(), "+".to_string());

                let stmt = iter.next().unwrap();
                let ident = Node::ExpressionStatement {
                    expression: Some(Box::new(Node::InfixExpression {
                        left: Box::new(Node::IntegerLiteral { value: 5 }),
                        operator: "-".to_string(),
                        right: Box::new(Node::IntegerLiteral { value: 5 }),
                    })),
                };
                assert_eq!(stmt, ident);
                assert_eq!(stmt.token_literal(), "-".to_string());

                let stmt = iter.next().unwrap();
                let ident = Node::ExpressionStatement {
                    expression: Some(Box::new(Node::InfixExpression {
                        left: Box::new(Node::IntegerLiteral { value: 5 }),
                        operator: "*".to_string(),
                        right: Box::new(Node::IntegerLiteral { value: 5 }),
                    })),
                };
                assert_eq!(stmt, ident);
                assert_eq!(stmt.token_literal(), "*".to_string());

                let stmt = iter.next().unwrap();
                let ident = Node::ExpressionStatement {
                    expression: Some(Box::new(Node::InfixExpression {
                        left: Box::new(Node::IntegerLiteral { value: 5 }),
                        operator: "/".to_string(),
                        right: Box::new(Node::IntegerLiteral { value: 5 }),
                    })),
                };
                assert_eq!(stmt, ident);
                assert_eq!(stmt.token_literal(), "/".to_string());

                let stmt = iter.next().unwrap();
                let ident = Node::ExpressionStatement {
                    expression: Some(Box::new(Node::InfixExpression {
                        left: Box::new(Node::IntegerLiteral { value: 5 }),
                        operator: ">".to_string(),
                        right: Box::new(Node::IntegerLiteral { value: 5 }),
                    })),
                };
                assert_eq!(stmt, ident);
                assert_eq!(stmt.token_literal(), ">".to_string());

                let stmt = iter.next().unwrap();
                let ident = Node::ExpressionStatement {
                    expression: Some(Box::new(Node::InfixExpression {
                        left: Box::new(Node::IntegerLiteral { value: 5 }),
                        operator: "<".to_string(),
                        right: Box::new(Node::IntegerLiteral { value: 5 }),
                    })),
                };
                assert_eq!(stmt, ident);
                assert_eq!(stmt.token_literal(), "<".to_string());

                let stmt = iter.next().unwrap();
                let ident = Node::ExpressionStatement {
                    expression: Some(Box::new(Node::InfixExpression {
                        left: Box::new(Node::IntegerLiteral { value: 5 }),
                        operator: "==".to_string(),
                        right: Box::new(Node::IntegerLiteral { value: 5 }),
                    })),
                };
                assert_eq!(stmt, ident);
                assert_eq!(stmt.token_literal(), "==".to_string());

                let stmt = iter.next().unwrap();
                let ident = Node::ExpressionStatement {
                    expression: Some(Box::new(Node::InfixExpression {
                        left: Box::new(Node::IntegerLiteral { value: 5 }),
                        operator: "!=".to_string(),
                        right: Box::new(Node::IntegerLiteral { value: 5 }),
                    })),
                };
                assert_eq!(stmt, ident);
                assert_eq!(stmt.token_literal(), "!=".to_string());

                let stmt = iter.next().unwrap();
                let ident = Node::ExpressionStatement {
                    expression: Some(Box::new(Node::InfixExpression {
                        left: Box::new(Node::Boolean { value: true }),
                        operator: "==".to_string(),
                        right: Box::new(Node::Boolean { value: true }),
                    })),
                };
                assert_eq!(stmt, ident);
                assert_eq!(stmt.token_literal(), "==".to_string());

                let stmt = iter.next().unwrap();
                let ident = Node::ExpressionStatement {
                    expression: Some(Box::new(Node::InfixExpression {
                        left: Box::new(Node::Boolean { value: true }),
                        operator: "!=".to_string(),
                        right: Box::new(Node::Boolean { value: false }),
                    })),
                };
                assert_eq!(stmt, ident);
                assert_eq!(stmt.token_literal(), "!=".to_string());

                let stmt = iter.next().unwrap();
                let ident = Node::ExpressionStatement {
                    expression: Some(Box::new(Node::InfixExpression {
                        left: Box::new(Node::Boolean { value: false }),
                        operator: "==".to_string(),
                        right: Box::new(Node::Boolean { value: false }),
                    })),
                };
                assert_eq!(stmt, ident);
                assert_eq!(stmt.token_literal(), "==".to_string());
            }
            _ => panic!("Unsupported node type"),
        }
    }

    #[test]
    fn test_boolean_expression() {
        let input = "true;
            false;";

        let lexer = Lexer::new(input);
        let mut parser = Parser::new(lexer);

        let program = parser.parse_program();
        assert!(!did_parser_fail(parser.errors));

        match program {
            Node::Program { statements } => {
                assert_eq!(2, statements.len());
                let mut iter = statements.iter();
                let stmt = iter.next().unwrap();
                let ident = Node::ExpressionStatement {
                    expression: Some(Box::new(Node::Boolean { value: true })),
                };
                assert_eq!(*stmt, ident);
                assert_eq!(stmt.token_literal(), "true".to_string());

                let stmt = iter.next().unwrap();
                let ident = Node::ExpressionStatement {
                    expression: Some(Box::new(Node::Boolean { value: false })),
                };
                assert_eq!(*stmt, ident);
                assert_eq!(stmt.token_literal(), "false".to_string());
            }
            _ => panic!("Unsupported node type"),
        }
    }

    #[test]
    fn test_if_expression() {
        let input = "if x < y { x };";

        let lexer = Lexer::new(input);
        let mut parser = Parser::new(lexer);

        let program = parser.parse_program();
        assert!(!did_parser_fail(parser.errors));

        match program {
            Node::Program { statements } => {
                assert_eq!(1, statements.len());
                let mut iter = statements.iter();
                let stmt = iter.next().unwrap();
                let ident = Node::ExpressionStatement {
                    expression: Some(Box::new(Node::IfExpression {
                        condition: Box::new(Node::InfixExpression {
                            left: Box::new(Node::Identifier {
                                value: Token::new(TokenType::Ident, "x".to_string()),
                            }),
                            operator: "<".to_string(),
                            right: Box::new(Node::Identifier {
                                value: Token::new(TokenType::Ident, "y".to_string()),
                            }),
                        }),
                        consequence: Box::new(Node::BlockStatement {
                            statements: vec![Node::ExpressionStatement {
                                expression: Some(Box::new(Node::Identifier {
                                    value: Token::new(TokenType::Ident, "x".to_string()),
                                })),
                            }],
                        }),
                        alternative: None,
                    })),
                };
                assert_eq!(*stmt, ident);
                assert_eq!(stmt.token_literal(), "if".to_string());
            }
            _ => panic!("Unsupported node type"),
        }
    }

    #[test]
    fn test_if_else_expression() {
        let input = "if x < y { x } else { y };";

        let lexer = Lexer::new(input);
        let mut parser = Parser::new(lexer);

        let program = parser.parse_program();
        assert!(!did_parser_fail(parser.errors));

        match program {
            Node::Program { statements } => {
                assert_eq!(1, statements.len());
                let mut iter = statements.iter();
                let stmt = iter.next().unwrap();
                let ident = Node::ExpressionStatement {
                    expression: Some(Box::new(Node::IfExpression {
                        condition: Box::new(Node::InfixExpression {
                            left: Box::new(Node::Identifier {
                                value: Token::new(TokenType::Ident, "x".to_string()),
                            }),
                            operator: "<".to_string(),
                            right: Box::new(Node::Identifier {
                                value: Token::new(TokenType::Ident, "y".to_string()),
                            }),
                        }),
                        consequence: Box::new(Node::BlockStatement {
                            statements: vec![Node::ExpressionStatement {
                                expression: Some(Box::new(Node::Identifier {
                                    value: Token::new(TokenType::Ident, "x".to_string()),
                                })),
                            }],
                        }),
                        alternative: Some(Box::new(Node::BlockStatement {
                            statements: vec![Node::ExpressionStatement {
                                expression: Some(Box::new(Node::Identifier {
                                    value: Token::new(TokenType::Ident, "y".to_string()),
                                })),
                            }],
                        })),
                    })),
                };
                assert_eq!(*stmt, ident);
                assert_eq!(stmt.token_literal(), "if".to_string());
            }
            _ => panic!("Unsupported node type"),
        }
    }

    #[test]
    fn test_function_literal_parsing() {
        let input = "fn(x, y) { x + y; };";

        let lexer = Lexer::new(input);
        let mut parser = Parser::new(lexer);

        let program = parser.parse_program();
        assert!(!did_parser_fail(parser.errors));

        match program {
            Node::Program { statements } => {
                assert_eq!(1, statements.len());
                let mut iter = statements.iter();
                let stmt = iter.next().unwrap();
                let ident = Node::ExpressionStatement {
                    expression: Some(Box::new(Node::FunctionLiteral {
                        parameters: vec![
                            Node::Identifier {
                                value: Token::new(TokenType::Ident, "x".to_string()),
                            },
                            Node::Identifier {
                                value: Token::new(TokenType::Ident, "y".to_string()),
                            },
                        ],
                        body: Box::new(Node::BlockStatement {
                            statements: vec![Node::ExpressionStatement {
                                expression: Some(Box::new(Node::InfixExpression {
                                    left: Box::new(Node::Identifier {
                                        value: Token::new(TokenType::Ident, "x".to_string()),
                                    }),
                                    operator: "+".to_string(),
                                    right: Box::new(Node::Identifier {
                                        value: Token::new(TokenType::Ident, "y".to_string()),
                                    }),
                                })),
                            }],
                        }),
                    })),
                };
                assert_eq!(*stmt, ident);
                assert_eq!(stmt.token_literal(), "fn".to_string());
            }
            _ => panic!("Unexpected node type"),
        }
    }

    #[test]
    fn test_function_parameter_parsing() {
        let table = vec![
            ("fn() {};", vec![]),
            ("fn(x) {};", vec!["x"]),
            ("fn(x, y) {};", vec!["x", "y"]),
        ];

        table.iter().for_each(|(input, output)| {
            let lexer = Lexer::new(input);
            let mut parser = Parser::new(lexer);

            let program = parser.parse_program();
            assert!(!did_parser_fail(parser.errors));

            match program {
                Node::Program { statements } => match &statements[0] {
                    Node::ExpressionStatement { expression } => {
                        match *expression
                            .clone()
                            .expect("found empty expression statement")
                        {
                            Node::FunctionLiteral { parameters, .. } => {
                                assert_eq!(parameters.len(), output.len());
                                parameters.iter().zip(output.iter()).for_each(|(p, o)| {
                                    assert_eq!(&p.token_literal(), *o);
                                });
                            }
                            _ => panic!("Unexpected node type"),
                        }
                    }
                    _ => panic!("Unexpected node type"),
                },
                _ => panic!("Unexpected node type"),
            }
        });
    }

    #[test]
    fn test_call_expression_parsing() {
        let input = "add(1, 2 * 3, 4 + 5);";

        let lexer = Lexer::new(input);
        let mut parser = Parser::new(lexer);

        let program = parser.parse_program();
        assert!(!did_parser_fail(parser.errors));

        match program {
            Node::Program { statements } => {
                let mut iter = statements.iter();
                let stmt = iter.next().unwrap();
                let ident = Node::ExpressionStatement {
                    expression: Some(Box::new(Node::CallExpression {
                        function: Box::new(Node::Identifier {
                            value: Token::new(TokenType::Ident, "add".to_string()),
                        }),
                        arguments: vec![
                            Node::IntegerLiteral { value: 1 },
                            Node::InfixExpression {
                                left: Box::new(Node::IntegerLiteral { value: 2 }),
                                operator: "*".to_string(),
                                right: Box::new(Node::IntegerLiteral { value: 3 }),
                            },
                            Node::InfixExpression {
                                left: Box::new(Node::IntegerLiteral { value: 4 }),
                                operator: "+".to_string(),
                                right: Box::new(Node::IntegerLiteral { value: 5 }),
                            },
                        ],
                    })),
                };
                assert_eq!(*stmt, ident);
                assert_eq!(stmt.token_literal(), "add".to_string());
            }
            _ => panic!("Unexpected node type"),
        }
    }

    #[test]
    fn test_call_expression_parameter_parsing() {
        let table = vec![
            ("add();", vec![]),
            ("add(x);", vec!["x"]),
            ("add(x, y);", vec!["x", "y"]),
        ];

        table.iter().for_each(|(input, output)| {
            let lexer = Lexer::new(input);
            let mut parser = Parser::new(lexer);

            let program = parser.parse_program();
            assert!(!did_parser_fail(parser.errors));

            match program {
                Node::Program { statements } => match &statements[0] {
                    Node::ExpressionStatement { expression } => {
                        match *expression
                            .clone()
                            .expect("found empty expression statement")
                        {
                            Node::CallExpression {
                                function: _,
                                arguments,
                            } => {
                                assert_eq!(arguments.len(), output.len());
                                arguments.iter().zip(output.iter()).for_each(|(a, o)| {
                                    assert_eq!(&a.token_literal(), *o);
                                });
                            }
                            _ => panic!("Unexpected node type"),
                        }
                    }
                    _ => panic!("Unexpected node type"),
                },
                _ => panic!("Unexpected node type"),
            }
        });
    }

    #[test]
    fn test_operator_precedence_parsing() {
        let table = vec![
            ("-a * b;", "((-a) * b);"),
            ("!-a;", "(!(-a));"),
            ("a + b + c;", "((a + b) + c);"),
            ("a + b - c;", "((a + b) - c);"),
            ("a * b * c;", "((a * b) * c);"),
            ("a * b / c;", "((a * b) / c);"),
            ("a + b / c;", "(a + (b / c));"),
            ("a + b * c + d / e - f;", "(((a + (b * c)) + (d / e)) - f);"),
            ("3 + 4; -5 * 5;", "(3 + 4);((-5) * 5);"),
            ("5 > 4 == 3 < 4;", "((5 > 4) == (3 < 4));"),
            ("5 < 4 != 3 > 4;", "((5 < 4) != (3 > 4));"),
            (
                "3 + 4 * 5 == 3 * 1 + 4 * 5;",
                "((3 + (4 * 5)) == ((3 * 1) + (4 * 5)));",
            ),
            (
                "3 + 4 * 5 == 3 * 1 + 4 * 5;",
                "((3 + (4 * 5)) == ((3 * 1) + (4 * 5)));",
            ),
            ("true;", "true;"),
            ("false;", "false;"),
            ("3 > 5 == false;", "((3 > 5) == false);"),
            ("3 < 5 == true;", "((3 < 5) == true);"),
            ("1 + (2 + 3) + 4;", "((1 + (2 + 3)) + 4);"),
            ("(5 + 5) * 2;", "((5 + 5) * 2);"),
            ("2 / (5 + 5);", "(2 / (5 + 5));"),
            ("-(5 + 5);", "(-(5 + 5));"),
            ("!(true == true);", "(!(true == true));"),
            ("a + add(b * c) + d;", "((a + add((b * c))) + d);"),
            (
                "add(a, b, 1, 2 * 3, 4 + 5, add(6, 7 * 8));",
                "add(a, b, 1, (2 * 3), (4 + 5), add(6, (7 * 8)));",
            ),
            (
                "add(a + b + c * d / f + g);",
                "add((((a + b) + ((c * d) / f)) + g));",
            ),
        ];

        table.iter().for_each(|(input, output)| {
            let lexer = Lexer::new(*input);
            let mut parser = Parser::new(lexer);

            let program = parser.parse_program();
            assert!(!did_parser_fail(parser.errors));

            assert_eq!(&program.as_string(), *output);
        });
    }

    #[test]
    fn test_parse_string() {
        let input = "
            !5 * 5 + 5 * 5;";

        let lexer = Lexer::new(input);
        let mut parser = Parser::new(lexer);

        let program = parser.parse_program();
        assert!(!did_parser_fail(parser.errors));

        println!("{}", program.as_string());
    }

    fn did_parser_fail(errors: Vec<ParserError>) -> bool {
        if errors.len() == 0 {
            false
        } else {
            errors.iter().for_each(|e| {
                println!("{:?}", e);
            });

            true
        }
    }
}
