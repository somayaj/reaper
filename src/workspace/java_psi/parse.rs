use super::imports::ImportMap;
use super::lexer::{Keyword, Token, TokenKind, lex};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompilationUnit {
    pub package: Option<String>,
    pub imports: ImportMap,
    pub types: Vec<TypeDecl>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDecl {
    pub kind: TypeKind,
    pub name: String,
    pub line: u32,
    pub column: u32,
    pub members: Vec<MemberDecl>,
    pub nested: Vec<TypeDecl>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
    Class,
    Interface,
    Enum,
    Record,
    Annotation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberDecl {
    pub kind: MemberKind,
    pub name: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberKind {
    Method,
    Field,
    Constructor,
}

pub fn parse_compilation_unit(content: &str) -> CompilationUnit {
    let tokens = lex(content);
    Parser { tokens: &tokens, pos: 0 }.parse_unit()
}

/// First declaration line/column for a top-level or nested type by simple name.
pub fn find_type_position(content: &str, simple: &str) -> (u32, u32) {
    fn walk(types: &[TypeDecl], simple: &str) -> Option<(u32, u32)> {
        for ty in types {
            if ty.name == simple {
                return Some((ty.line, ty.column));
            }
            if let Some(hit) = walk(&ty.nested, simple) {
                return Some(hit);
            }
        }
        None
    }
    walk(&parse_compilation_unit(content).types, simple).unwrap_or((1, 1))
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len().saturating_sub(1))]
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Eof)
    }

    fn bump(&mut self) {
        if !self.at_eof() {
            self.pos += 1;
        }
    }

    fn parse_unit(&mut self) -> CompilationUnit {
        let mut unit = CompilationUnit::default();
        while !self.at_eof() {
            if self.match_keyword(Keyword::Package) {
                let pkg = self.parse_qualified_name();
                unit.package = if pkg.is_empty() { None } else { Some(pkg) };
                self.expect_semi();
                continue;
            }
            if self.match_keyword(Keyword::Import) {
                self.parse_import(&mut unit.imports);
                continue;
            }
            if let Some(ty) = self.try_parse_type_decl(false) {
                unit.types.push(ty);
                continue;
            }
            self.bump();
        }
        unit
    }

    fn parse_import(&mut self, imports: &mut ImportMap) {
        let _static = self.match_keyword(Keyword::Static);
        let fqcn = self.parse_qualified_name();
        if self.match_kind(TokenKind::Dot) && self.match_kind(TokenKind::Star) {
            if !fqcn.is_empty() {
                imports.wildcards.push(fqcn);
            }
        } else if !fqcn.is_empty() {
            if let Some(simple) = fqcn.rsplit('.').next() {
                imports.explicit.insert(simple.to_string(), fqcn);
            }
        }
        self.expect_semi();
        let _ = _static;
    }

    fn try_parse_type_decl(&mut self, nested: bool) -> Option<TypeDecl> {
        let start = self.pos;
        self.skip_modifiers_and_annotations();
        let kind = if self.match_kind(TokenKind::At) {
            if !self.match_keyword(Keyword::Interface) {
                self.pos = start;
                return None;
            }
            TypeKind::Annotation
        } else if self.match_keyword(Keyword::Class) {
            TypeKind::Class
        } else if self.match_keyword(Keyword::Interface) {
            TypeKind::Interface
        } else if self.match_keyword(Keyword::Enum) {
            TypeKind::Enum
        } else if self.match_keyword(Keyword::Record) {
            TypeKind::Record
        } else {
            self.pos = start;
            return None;
        };

        let name = self.parse_type_name()?;
        self.skip_type_header_tail();

        let line = self.tokens.get(start).map(|t| t.line).unwrap_or(1);
        let column = self.tokens.get(start).map(|t| t.column).unwrap_or(1);

        let mut decl = TypeDecl {
            kind,
            name,
            line,
            column,
            members: Vec::new(),
            nested: Vec::new(),
        };

        if self.match_kind(TokenKind::LBrace) {
            self.parse_type_body(&mut decl);
        } else if nested {
            self.pos = start;
            return None;
        }

        Some(decl)
    }

    fn parse_type_body(&mut self, owner: &mut TypeDecl) {
        let mut depth = 1usize;
        while !self.at_eof() && depth > 0 {
            match self.peek().kind {
                TokenKind::RBrace => {
                    self.bump();
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                TokenKind::LBrace => {
                    self.bump();
                    depth += 1;
                }
                _ if depth == 1 => {
                    if let Some(nested) = self.try_parse_type_decl(true) {
                        owner.nested.push(nested);
                        continue;
                    }
                    if let Some(member) = self.try_parse_member() {
                        owner.members.push(member);
                        continue;
                    }
                    self.bump();
                }
                _ => self.bump(),
            }
        }
    }

    fn try_parse_member(&mut self) -> Option<MemberDecl> {
        let start = self.pos;
        self.skip_modifiers_and_annotations();
        let _ = self.skip_single_type();
        let name = match self.parse_identifier() {
            Some(n) => n,
            None => {
                self.pos = start;
                return None;
            }
        };

        if self.match_kind(TokenKind::LParen) {
            self.skip_balanced(TokenKind::LParen, TokenKind::RParen);
            self.skip_method_tail();
            let line = self.tokens.get(start).map(|t| t.line).unwrap_or(1);
            let col = self
                .tokens
                .iter()
                .skip(start)
                .find(|t| matches!(&t.kind, TokenKind::Identifier(id) if id == &name))
                .map(|t| t.column)
                .unwrap_or(1);
            return Some(MemberDecl {
                kind: if name.chars().next().is_some_and(|c| c.is_uppercase()) {
                    MemberKind::Constructor
                } else {
                    MemberKind::Method
                },
                name,
                line,
                column: col,
            });
        }

        if self.match_kind(TokenKind::Semi) {
            let col = self
                .tokens
                .iter()
                .skip(start)
                .find(|t| matches!(&t.kind, TokenKind::Identifier(id) if id == &name))
                .map(|t| t.column)
                .unwrap_or(1);
            let line = self.tokens.get(start).map(|t| t.line).unwrap_or(1);
            if is_keyword(&name) {
                return None;
            }
            return Some(MemberDecl {
                kind: MemberKind::Field,
                name,
                line,
                column: col,
            });
        }

        self.pos = start;
        None
    }

    fn skip_method_tail(&mut self) {
        while !self.at_eof() {
            match self.peek().kind {
                TokenKind::Semi => {
                    self.bump();
                    break;
                }
                TokenKind::LBrace => {
                    self.bump();
                    self.skip_balanced_depth(TokenKind::LBrace, TokenKind::RBrace);
                    break;
                }
                TokenKind::Keyword(Keyword::Throws) => {
                    self.bump();
                    self.skip_until_semi_or_brace();
                }
                _ => self.bump(),
            }
        }
    }

    fn skip_until_semi_or_brace(&mut self) {
        while !self.at_eof() {
            match self.peek().kind {
                TokenKind::Semi | TokenKind::LBrace => break,
                _ => self.bump(),
            }
        }
    }

    fn skip_type_header_tail(&mut self) {
        while !self.at_eof() {
            match self.peek().kind {
                TokenKind::LBrace | TokenKind::Semi => break,
                TokenKind::Lt => {
                    self.bump();
                    self.skip_balanced(TokenKind::Lt, TokenKind::Gt);
                }
                _ => self.bump(),
            }
        }
    }

    fn skip_single_type(&mut self) -> bool {
        let saw = match self.peek().kind.clone() {
            TokenKind::Keyword(Keyword::Void) => {
                self.bump();
                true
            }
            TokenKind::Identifier(_) => {
                self.bump();
                while matches!(self.peek().kind, TokenKind::Dot) {
                    if !self
                        .tokens
                        .get(self.pos + 1)
                        .is_some_and(|t| matches!(t.kind, TokenKind::Identifier(_)))
                    {
                        break;
                    }
                    self.bump();
                    self.bump();
                }
                true
            }
            _ => return false,
        };
        while matches!(self.peek().kind, TokenKind::Lt) {
            self.bump();
            self.skip_balanced(TokenKind::Lt, TokenKind::Gt);
        }
        while self.match_kind(TokenKind::LBracket) {
            let _ = self.match_kind(TokenKind::RBracket);
        }
        saw
    }

    #[allow(dead_code)]
    fn skip_type_tokens(&mut self) -> bool {
        self.skip_single_type()
    }

    fn skip_modifiers_and_annotations(&mut self) {
        loop {
            if self.match_kind(TokenKind::At) {
                if matches!(self.peek().kind, TokenKind::Identifier(_)) {
                    self.bump();
                }
                if self.match_kind(TokenKind::LParen) {
                    self.skip_balanced(TokenKind::LParen, TokenKind::RParen);
                }
                continue;
            }
            if matches!(
                self.peek().kind,
                TokenKind::Keyword(
                    Keyword::Public
                        | Keyword::Private
                        | Keyword::Protected
                        | Keyword::Static
                        | Keyword::Final
                        | Keyword::Abstract
                        | Keyword::Sealed
                        | Keyword::NonSealed
                )
            ) {
                self.bump();
                continue;
            }
            break;
        }
    }

    fn skip_balanced(&mut self, open: TokenKind, close: TokenKind) {
        if !self.match_kind(open.clone()) {
            return;
        }
        self.skip_balanced_depth(open, close);
    }

    fn skip_balanced_depth(&mut self, open: TokenKind, close: TokenKind) {
        let mut depth = 1usize;
        while !self.at_eof() && depth > 0 {
            if self.match_kind(open.clone()) {
                depth += 1;
            } else if self.match_kind(close.clone()) {
                depth -= 1;
            } else {
                self.bump();
            }
        }
    }

    fn parse_type_name(&mut self) -> Option<String> {
        self.parse_identifier()
    }

    fn parse_identifier(&mut self) -> Option<String> {
        match self.peek().kind.clone() {
            TokenKind::Identifier(name) => {
                self.bump();
                Some(name)
            }
            _ => None,
        }
    }

    fn parse_qualified_name(&mut self) -> String {
        let mut parts = Vec::new();
        if let Some(first) = self.parse_identifier() {
            parts.push(first);
        }
        while matches!(self.peek().kind, TokenKind::Dot) {
            let next_is_ident = self
                .tokens
                .get(self.pos + 1)
                .is_some_and(|t| matches!(t.kind, TokenKind::Identifier(_)));
            if !next_is_ident {
                break;
            }
            self.bump(); // dot
            if let Some(part) = self.parse_identifier() {
                parts.push(part);
            } else {
                break;
            }
        }
        parts.join(".")
    }

    fn expect_semi(&mut self) {
        let _ = self.match_kind(TokenKind::Semi);
    }

    fn match_keyword(&mut self, kw: Keyword) -> bool {
        if matches!(self.peek().kind, TokenKind::Keyword(k) if k == kw) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn match_kind(&mut self, kind: TokenKind) -> bool {
        if self.at_eof() {
            return false;
        }
        let same = match (&self.peek().kind, &kind) {
            (TokenKind::Identifier(_), TokenKind::Identifier(_)) => true,
            (a, b) => a == b,
        };
        if same {
            self.bump();
        }
        same
    }
}

fn is_keyword(name: &str) -> bool {
    super::super::symbols::is_keyword(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_package_imports_and_class() {
        let unit = parse_compilation_unit(
            "package com.example;\n\
             import org.springframework.web.bind.annotation.*;\n\
             import org.springframework.web.bind.annotation.RestController;\n\
             public class App {}",
        );
        assert_eq!(unit.package.as_deref(), Some("com.example"));
        assert_eq!(
            unit.imports.explicit.get("RestController"),
            Some(&"org.springframework.web.bind.annotation.RestController".to_string())
        );
        assert_eq!(
            unit.imports.wildcards,
            vec!["org.springframework.web.bind.annotation".to_string()]
        );
        assert_eq!(unit.types.len(), 1);
        assert_eq!(unit.types[0].name, "App");
    }

    #[test]
    fn ignores_class_in_string_literal() {
        let unit = parse_compilation_unit(
            r#"public class Real { String s = "class Fake {}"; }"#,
        );
        assert_eq!(unit.types.len(), 1);
        assert_eq!(unit.types[0].name, "Real");
    }

    #[test]
    fn parses_nested_type() {
        let unit = parse_compilation_unit(
            "public class Outer { static class Inner { void run() {} } }",
        );
        assert_eq!(unit.types[0].nested.len(), 1);
        assert_eq!(unit.types[0].nested[0].name, "Inner");
        assert!(unit.types[0].nested[0]
            .members
            .iter()
            .any(|m| m.name == "run"));
    }

    #[test]
    fn finds_type_position_by_name() {
        let src = "package p;\npublic class Alpha { class Beta {} }";
        assert_eq!(find_type_position(src, "Alpha"), (2, 1));
        let beta = find_type_position(src, "Beta");
        assert_eq!(beta.0, 2);
        assert!(beta.1 > 1);
    }
}
