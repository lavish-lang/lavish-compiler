use heck::SnakeCase;
use indexmap::IndexMap;
use std::fmt::{self, Display, Write};

use super::output::Scope;
use crate::ast;
use crate::codegen::Result;

pub trait WriteTo: fmt::Display {
    fn write_to(&self, s: &mut Scope) {
        write!(s, "{}", self).unwrap();
    }
}

impl<T> WriteTo for T where T: fmt::Display {}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum FunKind {
    Request,
    Notification,
}

pub struct Namespace<'a> {
    name: &'a str,

    children: IndexMap<&'a str, Namespace<'a>>,
    funs: IndexMap<&'a str, Fun<'a>>,
    strus: IndexMap<&'a str, Stru<'a>>,
}

impl<'a> Namespace<'a> {
    pub fn new(prefix: &str, name: &'a str, decl: &'a ast::NamespaceBody) -> Self {
        let prefix = if name == "<root>" {
            "".into()
        } else {
            format!("{}{}.", prefix, name)
        };

        let mut children: IndexMap<&'a str, Namespace<'a>> = IndexMap::new();
        let mut funs: IndexMap<&'a str, Fun<'a>> = IndexMap::new();
        let mut strus: IndexMap<&'a str, Stru<'a>> = IndexMap::new();

        for decl in &decl.functions {
            let ff = Fun::new(&prefix, decl);
            funs.insert(&decl.name.text, ff);
        }

        for decl in &decl.structs {
            let full_name = format!("{}{}", prefix, decl.name.text);
            let st = Stru::new(decl, full_name);
            strus.insert(&decl.name.text, st);
        }

        for decl in &decl.namespaces {
            let name = decl.name.text.as_ref();
            children.insert(name, Namespace::new(&prefix, name, &decl.body));
        }

        Namespace {
            name,
            children,
            funs,
            strus,
        }
    }

    pub fn funs(&self) -> Box<Iterator<Item = &'a Fun> + 'a> {
        Box::new(
            self.children
                .values()
                .map(Namespace::funs)
                .flatten()
                .chain(self.funs.values().map(|f| f.funs()).flatten()),
        )
    }

    pub fn local_funs(&'a self) -> impl Iterator<Item = &'a Fun> {
        self.funs.values()
    }

    pub fn name(&self) -> &'a str {
        self.name
    }

    pub fn children(&self) -> &IndexMap<&'a str, Namespace<'a>> {
        &self.children
    }

    pub fn strus(&self) -> &IndexMap<&'a str, Stru<'a>> {
        &self.strus
    }
}

pub enum FunStructKind {
    Params,
    Results,
}

pub struct FunStruct<'a> {
    pub fun: &'a Fun<'a>,
    pub kind: FunStructKind,
    pub fields: &'a Vec<ast::Field>,
}

impl<'a> FunStruct<'a> {
    pub fn kind(&self) -> &str {
        match self.kind {
            FunStructKind::Params => "Params",
            FunStructKind::Results => "Results",
        }
    }

    pub fn variant(&self) -> String {
        format!("{}::{}", self.kind(), self.fun.variant())
    }

    pub fn qualified_type(&self) -> String {
        format!("{}::{}", self.fun.qualified_name(), self.kind())
    }

    pub fn short_type(&self) -> String {
        if self.is_empty() {
            "()".into()
        } else {
            self.qualified_type()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn fields(&self) -> &Vec<ast::Field> {
        self.fields
    }

    pub fn empty_literal(&self) -> String {
        format!("{} {{}}", self.qualified_type())
    }
}

pub struct Derive {
    items: Vec<&'static str>,
}

impl Derive {
    pub fn debug(mut self) -> Self {
        self.items.push("Debug");
        self
    }

    pub fn serialize(mut self) -> Self {
        self.items.push("lavish_rpc::serde_derive::Serialize");
        self
    }

    pub fn deserialize(mut self) -> Self {
        self.items.push("lavish_rpc::serde_derive::Deserialize");
        self
    }
}

impl Display for Derive {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "#[derive({items})]", items = self.items.join(", "))
    }
}

pub fn derive() -> Derive {
    Derive { items: Vec::new() }
}

pub struct Allow {
    items: Vec<&'static str>,
}

impl Allow {
    pub fn non_camel_case(mut self) -> Self {
        self.items.push("non_camel_case_types");
        self
    }

    pub fn unused(mut self) -> Self {
        self.items.push("unused");
        self
    }
}

pub struct _Fn<'a> {
    kw_pub: bool,
    kw_async: bool,
    self_arg: Option<String>,
    name: String,
    ret: Option<String>,
    body: Option<&'a Fn(&mut Scope) -> Result>,
}

impl<'a> _Fn<'a> {
    pub fn kw_pub(mut self) -> Self {
        self.kw_pub = true;
        self
    }

    pub fn kw_async(mut self) -> Self {
        self.kw_async = true;
        self
    }

    pub fn returns<D>(mut self, ret: D) -> Self
    where
        D: Display,
    {
        self.ret = Some(format!("{}", ret));
        self
    }

    pub fn body(mut self, f: &'a Fn(&mut Scope) -> Result) -> Self {
        self.body = Some(f);
        self
    }

    pub fn self_param<D>(mut self, self_arg: D) -> Self
    where
        D: Display,
    {
        self.self_arg = Some(format!("{}", self_arg));
        self
    }
}

impl<'a> Display for _Fn<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Scope::fmt(f, |s| {
            if self.kw_pub {
                s.write("pub ");
            }
            if self.kw_async {
                s.write("async ");
            }

            s.write("fn ").write(&self.name);
            // TODO: write type parameters

            s.write("(");
            let mut _prev_arg = false;
            if let Some(self_param) = self.self_arg.as_ref() {
                s.write(self_param);
                _prev_arg = true;
            }
            // TODO: write args
            s.write(")");

            if let Some(ret) = self.ret.as_ref() {
                s.write(" -> ").write(ret);
            }

            // TODO: write where clauses
            if let Some(body) = self.body.as_ref() {
                s.in_block(|s| {
                    body(s)?;
                    Ok(())
                })?;
            } else {
                s.write(";").lf();
            }

            Ok(())
        })
    }
}

pub fn _fn<'a, N>(name: N) -> _Fn<'a>
where
    N: Into<String>,
{
    _Fn {
        kw_pub: false,
        kw_async: false,
        name: name.into(),
        self_arg: None,
        body: None,
        ret: None,
    }
}

impl Display for Allow {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "#[allow({items})]", items = self.items.join(", "))
    }
}

pub fn allow() -> Allow {
    Allow { items: Vec::new() }
}

pub fn serde_untagged() -> impl Display {
    "#[serde(untagged)]\n"
}

pub struct Atom<'a> {
    pub proto: &'a Protocol<'a>,
    pub name: &'a str,
    pub kind: FunKind,
    pub depth: usize,
}

impl<'a> Atom<'a> {
    fn funs(&self) -> impl Iterator<Item = &&Fun> {
        let kind = self.kind;
        self.proto.funs.iter().filter(move |f| f.kind() == kind)
    }
}

impl<'a> Atom<'a> {
    fn root(&self) -> String {
        "super::".repeat(self.depth)
    }
}

impl<'a> Display for Atom<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Scope::fmt(f, |s| {
            s.write(derive().debug().serialize());
            s.write(allow().non_camel_case().unused());
            s.write(serde_untagged());
            s.write(Block::Enum(self.name, |s| {
                for fun in self.funs() {
                    s.write(fun.variant())
                        .write("(")
                        .write(self.root())
                        .write(fun.qualified_name())
                        .write("::")
                        .write(&self.name)
                        .write("),")
                        .lf();
                }
                Ok(())
            }));

            s.line(Block::Impl("lavish_rpc::Atom", self.name, |s| {
                _fn("method")
                    .self_param("&self")
                    .returns("&'static str")
                    .body(&|s| {
                        s.line("// TODO");
                        s.write("match self");
                        s.in_block(|s| {
                            for fun in self.funs() {
                                s.write(&self.name)
                                    .write("::")
                                    .write(fun.variant())
                                    .write("(_)");
                                writeln!(s, " => {:?},", fun.rpc_name())?;
                            }
                            Ok(())
                        })?;
                        Ok(())
                    })
                    .write_to(s);

                _fn("deserialize")
                    .returns("erased_serde::Result<Self>")
                    .body(&|s| {
                        s.line("unimplemented!()");
                        Ok(())
                    })
                    .write_to(s);
                Ok(())
            }));
            Ok(())
        })
    }
}

pub struct Block<F>
where
    F: Fn(&mut Scope) -> Result,
{
    prefix: String,
    f: F,
}

impl<F> Display for Block<F>
where
    F: Fn(&mut Scope) -> Result,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Scope::fmt(f, |s| {
            write!(s, "{prefix}", prefix = self.prefix)?;
            s.in_block(|s| (self.f)(s))
        })
    }
}

#[allow(non_snake_case)]
impl<F> Block<F>
where
    F: Fn(&mut Scope) -> Result,
{
    pub fn Mod<N>(name: N, f: F) -> Self
    where
        N: Display,
    {
        Self {
            prefix: format!("pub mod {name}", name = name),
            f,
        }
    }

    pub fn Enum<N>(name: N, f: F) -> Self
    where
        N: Display,
    {
        Self {
            prefix: format!("pub enum {name}", name = name),
            f,
        }
    }

    pub fn Impl<T, N>(trt: T, name: N, f: F) -> Self
    where
        T: Display,
        N: Display,
    {
        Self {
            prefix: format!("impl {trt} for {name}", trt = trt, name = name),
            f,
        }
    }
}

pub struct Protocol<'a> {
    pub funs: &'a [&'a Fun<'a>],
    pub depth: usize,
}

impl<'a> Display for Protocol<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let b = Block::Mod("protocol", |s| {
            let depth = self.depth + 1;
            for a in &[
                Atom {
                    proto: &self,
                    kind: FunKind::Request,
                    name: "Params",
                    depth,
                },
                Atom {
                    proto: &self,
                    kind: FunKind::Request,
                    name: "Results",
                    depth,
                },
                Atom {
                    proto: &self,
                    kind: FunKind::Notification,
                    name: "NotificationParams",
                    depth,
                },
            ] {
                writeln!(s, "{}\n", a)?;
            }

            Ok(())
        });
        writeln!(f, "{}", b)
    }
}

pub struct Fun<'a> {
    decl: &'a ast::FunctionDecl,
    tokens: Vec<String>,

    body: Option<Namespace<'a>>,
}

impl<'a> Fun<'a> {
    pub fn new(prefix: &str, decl: &'a ast::FunctionDecl) -> Self {
        let name: &str = &decl.name.text;
        let full_name = format!("{}{}", prefix, name);
        Self {
            decl,
            tokens: full_name.split('.').map(|x| x.into()).collect(),
            body: decl.body.as_ref().map(|b| Namespace::new(prefix, name, b)),
        }
    }

    pub fn has_modifier(&self, modif: ast::FunctionModifier) -> bool {
        self.decl.modifiers.contains(&modif)
    }

    pub fn rpc_name(&self) -> String {
        self.tokens.join(".")
    }

    pub fn variant(&self) -> String {
        self.rpc_name().replace(".", "__").to_lowercase()
    }

    pub fn params(&'a self) -> FunStruct<'a> {
        FunStruct {
            fun: self,
            fields: &self.decl.params,
            kind: FunStructKind::Params,
        }
    }

    pub fn results(&'a self) -> FunStruct<'a> {
        FunStruct {
            fun: self,
            fields: &self.decl.results,
            kind: FunStructKind::Results,
        }
    }

    pub fn qualified_name(&self) -> String {
        self.tokens.join("::")
    }

    pub fn mod_name(&self) -> String {
        self.decl.name.text.to_snake_case()
    }

    pub fn is_notification(&self) -> bool {
        self.decl
            .modifiers
            .contains(&ast::FunctionModifier::Notification)
    }

    pub fn kind(&self) -> FunKind {
        if self.is_notification() {
            FunKind::Notification
        } else {
            FunKind::Request
        }
    }

    pub fn comment(&self) -> &Option<ast::Comment> {
        &self.decl.comment
    }

    pub fn funs(&self) -> Box<Iterator<Item = &'a Fun> + 'a> {
        let iter = std::iter::once(self);
        if let Some(body) = self.body.as_ref() {
            Box::new(iter.chain(body.funs()))
        } else {
            Box::new(iter)
        }
    }

    pub fn body(&self) -> Option<&Namespace<'a>> {
        self.body.as_ref()
    }
}

pub struct Stru<'a> {
    decl: &'a ast::StructDecl,
    #[allow(unused)]
    full_name: String,
}

impl<'a> Stru<'a> {
    pub fn new(decl: &'a ast::StructDecl, full_name: String) -> Self {
        Self { decl, full_name }
    }

    pub fn comment(&self) -> &Option<ast::Comment> {
        &self.decl.comment
    }

    pub fn name(&self) -> &str {
        &self.decl.name.text
    }

    pub fn fields(&self) -> &Vec<ast::Field> {
        &self.decl.fields
    }
}
