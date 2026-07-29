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
    pub modifiers: Vec<String>,
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
    pub modifiers: Vec<String>,
    /// Field type or method return type (`void`, `int`, `String`, …).
    pub type_name: Option<String>,
    /// Parameter list text including parentheses, e.g. `(String name)`.
    pub params: Option<String>,
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
        let modifiers = self.collect_modifiers_and_annotations();
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
            modifiers,
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
                    if let Some(member) = self.try_parse_member(&owner.name) {
                        owner.members.push(member);
                        continue;
                    }
                    self.bump();
                }
                _ => self.bump(),
            }
        }
    }

    fn try_parse_member(&mut self, owner_name: &str) -> Option<MemberDecl> {
        let start = self.pos;
        let modifiers = self.collect_modifiers_and_annotations();
        let line = self.tokens.get(start).map(|t| t.line).unwrap_or(1);

        // Constructor: only when name matches the enclosing type (never annotations).
        if let TokenKind::Identifier(ctor_name) = self.peek().kind.clone() {
            let next_paren = self
                .tokens
                .get(self.pos + 1)
                .is_some_and(|t| matches!(t.kind, TokenKind::LParen));
            if next_paren && ctor_name == owner_name {
                let column = self.peek().column;
                self.bump();
                let params = self.consume_param_list_text();
                self.skip_method_tail();
                return Some(MemberDecl {
                    kind: MemberKind::Constructor,
                    name: ctor_name,
                    modifiers,
                    type_name: None,
                    params: Some(params),
                    line,
                    column,
                });
            }
        }

        let type_name = match self.parse_type_text() {
            Some(t) => t,
            None => {
                self.pos = start;
                return None;
            }
        };
        let name = match self.parse_identifier() {
            Some(n) => n,
            None => {
                self.pos = start;
                return None;
            }
        };
        if is_keyword(&name) {
            self.pos = start;
            return None;
        }
        let column = self
            .tokens
            .iter()
            .skip(start)
            .find(|t| matches!(&t.kind, TokenKind::Identifier(id) if id == &name))
            .map(|t| t.column)
            .unwrap_or(1);

        if matches!(self.peek().kind, TokenKind::LParen) {
            let params = self.consume_param_list_text();
            self.skip_method_tail();
            return Some(MemberDecl {
                kind: MemberKind::Method,
                name,
                modifiers,
                type_name: Some(type_name),
                params: Some(params),
                line,
                column,
            });
        }

        // Field: type name [= …];
        if matches!(
            self.peek().kind,
            TokenKind::Semi | TokenKind::Assign | TokenKind::Comma
        ) {
            self.skip_until_semi();
            let _ = self.match_kind(TokenKind::Semi);
            return Some(MemberDecl {
                kind: MemberKind::Field,
                name,
                modifiers,
                type_name: Some(type_name),
                params: None,
                line,
                column,
            });
        }

        self.pos = start;
        None
    }

    fn consume_param_list_text(&mut self) -> String {
        if !matches!(self.peek().kind, TokenKind::LParen) {
            return "()".into();
        }
        let start = self.pos;
        self.bump();
        self.skip_balanced_depth(TokenKind::LParen, TokenKind::RParen);
        let end = self.pos;
        let mut out = String::new();
        let mut prev_word = false;
        for tok in &self.tokens[start..end] {
            match &tok.kind {
                TokenKind::LParen => {
                    out.push('(');
                    prev_word = false;
                }
                TokenKind::RParen => {
                    out.push(')');
                    prev_word = false;
                }
                TokenKind::Comma => {
                    out.push_str(", ");
                    prev_word = false;
                }
                TokenKind::Dot => {
                    out.push('.');
                    prev_word = false;
                }
                TokenKind::Lt => {
                    out.push('<');
                    prev_word = false;
                }
                TokenKind::Gt => {
                    out.push('>');
                    prev_word = false;
                }
                TokenKind::LBracket => {
                    out.push('[');
                    prev_word = false;
                }
                TokenKind::RBracket => {
                    out.push(']');
                    prev_word = false;
                }
                TokenKind::Identifier(id) => {
                    if prev_word {
                        out.push(' ');
                    }
                    out.push_str(id);
                    prev_word = true;
                }
                TokenKind::Keyword(kw) => {
                    if prev_word {
                        out.push(' ');
                    }
                    out.push_str(keyword_label(*kw));
                    prev_word = true;
                }
                _ => {}
            }
        }
        if out.is_empty() {
            "()".into()
        } else {
            out
        }
    }

    fn skip_until_semi(&mut self) {
        while !self.at_eof() {
            match self.peek().kind {
                TokenKind::Semi => break,
                TokenKind::LBrace => {
                    self.bump();
                    self.skip_balanced_depth(TokenKind::LBrace, TokenKind::RBrace);
                }
                TokenKind::LParen => {
                    self.bump();
                    self.skip_balanced_depth(TokenKind::LParen, TokenKind::RParen);
                }
                _ => self.bump(),
            }
        }
    }

    fn parse_type_text(&mut self) -> Option<String> {
        let mut out = String::new();
        match self.peek().kind.clone() {
            TokenKind::Keyword(Keyword::Void) => {
                self.bump();
                out.push_str("void");
            }
            TokenKind::Identifier(id) => {
                self.bump();
                out.push_str(&id);
                while matches!(self.peek().kind, TokenKind::Dot) {
                    let next_is_ident = self
                        .tokens
                        .get(self.pos + 1)
                        .is_some_and(|t| matches!(t.kind, TokenKind::Identifier(_)));
                    if !next_is_ident {
                        break;
                    }
                    self.bump();
                    if let Some(part) = self.parse_identifier() {
                        out.push('.');
                        out.push_str(&part);
                    } else {
                        break;
                    }
                }
            }
            _ => return None,
        }
        while matches!(self.peek().kind, TokenKind::Lt) {
            out.push_str(&self.consume_angle_text());
        }
        while self.match_kind(TokenKind::LBracket) {
            let _ = self.match_kind(TokenKind::RBracket);
            out.push_str("[]");
        }
        Some(out)
    }

    fn consume_angle_text(&mut self) -> String {
        if !matches!(self.peek().kind, TokenKind::Lt) {
            return String::new();
        }
        self.bump();
        self.skip_balanced_depth(TokenKind::Lt, TokenKind::Gt);
        "<…>".into()
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

    fn collect_modifiers_and_annotations(&mut self) -> Vec<String> {
        let mut mods = Vec::new();
        loop {
            if self.match_kind(TokenKind::At) {
                // Skip annotations in the outline (keep modifiers only).
                // Support @Name, @pkg.Name, and @Name(…) / @Name(value = "…").
                if matches!(self.peek().kind, TokenKind::Identifier(_)) {
                    self.bump();
                    while matches!(self.peek().kind, TokenKind::Dot)
                        && self
                            .tokens
                            .get(self.pos + 1)
                            .is_some_and(|t| matches!(t.kind, TokenKind::Identifier(_)))
                    {
                        self.bump(); // dot
                        self.bump(); // ident
                    }
                }
                if self.match_kind(TokenKind::LParen) {
                    // '(' already consumed — do not call skip_balanced (it would re-match '(').
                    self.skip_balanced_depth(TokenKind::LParen, TokenKind::RParen);
                }
                continue;
            }
            if let TokenKind::Keyword(kw) = self.peek().kind {
                if matches!(
                    kw,
                    Keyword::Public
                        | Keyword::Private
                        | Keyword::Protected
                        | Keyword::Static
                        | Keyword::Final
                        | Keyword::Abstract
                        | Keyword::Sealed
                        | Keyword::NonSealed
                ) {
                    mods.push(keyword_label(kw).into());
                    self.bump();
                    continue;
                }
            }
            break;
        }
        mods
    }

    fn skip_modifiers_and_annotations(&mut self) {
        let _ = self.collect_modifiers_and_annotations();
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

fn keyword_label(kw: Keyword) -> &'static str {
    match kw {
        Keyword::Package => "package",
        Keyword::Import => "import",
        Keyword::Class => "class",
        Keyword::Interface => "interface",
        Keyword::Enum => "enum",
        Keyword::Record => "record",
        Keyword::Static => "static",
        Keyword::Extends => "extends",
        Keyword::Implements => "implements",
        Keyword::Throws => "throws",
        Keyword::Void => "void",
        Keyword::Return => "return",
        Keyword::New => "new",
        Keyword::Public => "public",
        Keyword::Private => "private",
        Keyword::Protected => "protected",
        Keyword::Abstract => "abstract",
        Keyword::Final => "final",
        Keyword::Sealed => "sealed",
        Keyword::NonSealed => "non-sealed",
        Keyword::Permits => "permits",
    }
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

    #[test]
    fn parses_modifiers_methods_fields_and_ctors() {
        let unit = parse_compilation_unit(
            "package com.example;\n\
             public final class Hello {\n\
               private static final int COUNT = 1;\n\
               public Hello(String name) {}\n\
               protected void run() {}\n\
             }",
        );
        assert_eq!(unit.package.as_deref(), Some("com.example"));
        let ty = &unit.types[0];
        assert_eq!(ty.name, "Hello");
        assert_eq!(ty.modifiers, vec!["public", "final"]);
        assert!(ty.members.iter().any(|m| {
            m.kind == MemberKind::Field
                && m.name == "COUNT"
                && m.modifiers.iter().any(|x| x == "private")
                && m.type_name.as_deref() == Some("int")
        }));
        assert!(ty.members.iter().any(|m| {
            m.kind == MemberKind::Constructor
                && m.name == "Hello"
                && m.params.as_deref() == Some("(String name)")
        }));
        assert!(ty.members.iter().any(|m| {
            m.kind == MemberKind::Method
                && m.name == "run"
                && m.modifiers.iter().any(|x| x == "protected")
                && m.type_name.as_deref() == Some("void")
        }));
    }

    #[test]
    fn annotations_are_not_parsed_as_constructors() {
        let unit = parse_compilation_unit(
            "package com.example.helloworld;\n\
             import org.springframework.web.bind.annotation.GetMapping;\n\
             import org.springframework.web.bind.annotation.RestController;\n\
             @RestController\n\
             public class HelloController {\n\
               @GetMapping(\"/\")\n\
               public String hello() { return \"hello world\"; }\n\
               @GetMapping(\"/root\")\n\
               public String helloRoot() { return \"com.example.helloworld\"; }\n\
             }",
        );
        assert_eq!(unit.types.len(), 1);
        let ty = &unit.types[0];
        assert_eq!(ty.name, "HelloController");
        assert!(
            !ty.members.iter().any(|m| m.name == "GetMapping"),
            "annotation names must not become members: {:?}",
            ty.members
        );
        assert_eq!(ty.members.len(), 2);
        assert!(ty.members.iter().any(|m| {
            m.kind == MemberKind::Method
                && m.name == "hello"
                && m.modifiers.iter().any(|x| x == "public")
                && m.type_name.as_deref() == Some("String")
        }));
        assert!(ty.members.iter().any(|m| {
            m.kind == MemberKind::Method
                && m.name == "helloRoot"
                && m.type_name.as_deref() == Some("String")
        }));
    }
}
