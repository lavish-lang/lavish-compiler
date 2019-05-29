mod rust;
use super::ast;

use std::fmt;

#[derive(Debug)]
pub struct Error {
    message: String,
}

impl From<std::fmt::Error> for Error {
    fn from(_: std::fmt::Error) -> Self {
        Self {
            message: "formatting error".into(),
        }
    }
}

impl Into<std::fmt::Error> for Error {
    fn into(self) -> std::fmt::Error {
        std::fmt::Error {}
    }
}

pub type Result = std::result::Result<(), Error>;

pub trait Generator {
    fn emit_workspace(&self, workspace: &ast::Workspace) -> Result;
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self {
            message: format!("i/o error: {}", e),
        }
    }
}

impl From<&'static str> for Error {
    fn from(s: &'static str) -> Self {
        Self { message: s.into() }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "codegen error: {}", self.message)
    }
}

pub fn codegen(workspace: &ast::Workspace) -> Result {
    let generator = match &workspace.rules.target {
        ast::Target::Rust(target) => rust::Generator::new(target.clone()),
        _ => panic!("Unimplemented target: {:#?}", workspace.rules.target),
    };

    generator.emit_workspace(&workspace)?;

    println!("Codegen done!");
    Ok(())
}
