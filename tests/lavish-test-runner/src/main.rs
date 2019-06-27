use colored::*;
use std::process::{self, Command};
use std::{env, fs, path};

fn status<S: Into<String>>(s: S) {
    println!("{}", s.into().blue());
}

fn task<S: Into<String>>(s: S) {
    println!("{}", s.into().yellow());
}

struct Context {
    tests_dir: path::PathBuf,
    compiler_path: path::PathBuf,
}

fn main() {
    std::env::set_var("RUST_BACKTRACE", "1");

    let cwd = env::current_dir().unwrap();
    let tests_dir = cwd.join("tests");

    if !tests_dir.exists() {
        panic!(
            "test dir {:?} does not exist (wrong working directory?)",
            tests_dir
        )
    }

    status("Building lavish compiler...");
    Command::new("cargo").args(&["build"]).run_verbose();

    let context = Context {
        tests_dir,
        compiler_path: cwd.join("target").join("debug").join("lavish"),
    };

    context.run_codegen_tests();
    context.run_compliance_tests();

    status("All done!")
}

const RUST_CODEGEN_CARGO_TEMPLATE: &str = r#"
[package]
name = "test"
version = "0.1.0"
edition = "2018"

[dependencies.lavish]
git = "https://github.com/lavish-lang/lavish-rs"
rev = "master"
"#;

const RUST_CODEGEN_LAVISH_RULES_TEMPLATE: &str = r#"
target rust {
    wrapper = lib
}

{{BUILDS}}
"#;

struct CodegenCase {
    name: String,
    schema_path: path::PathBuf,
}

#[derive(Debug)]
struct ComplianceCase {
    language: String,
    exec: String,
    args: Vec<String>,
}

struct ServerInstance {
    address: String,
    child: process::Child,
}

impl ComplianceCase {
    fn spawn_server(&self) -> ServerInstance {
        let mut args = self.args.clone();
        args.push("server".into());
        let mut child = Command::new(&self.exec)
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped())
            .stderr(process::Stdio::piped())
            .args(args)
            .spawn()
            .expect("launch server");

        status("Server spawned, reading address...");
        use std::io::{BufRead, BufReader};
        let address = {
            BufReader::new(child.stdout.as_mut().unwrap())
                .lines()
                .next()
                .expect("read lines from server")
                .expect("server writes non-empty line")
        };
        status(format!("Got address: {}", address));

        ServerInstance { child, address }
    }

    fn run_client(&self, server: &ServerInstance) {
        let mut args = self.args.clone();
        args.push("client".into());
        args.push(server.address.clone());
        let status = Command::new(&self.exec)
            .args(args)
            .status()
            .expect("launch client");
        if !status.success() {
            println!(
                "{}",
                format!("Client failed to run, exited with {:?}", status.code()).red()
            );
            process::exit(1);
        }
    }
}

impl Context {
    fn run_codegen_tests(&self) {
        let codegen_dir = self.tests_dir.join("codegen");
        task("Codegen tests");

        let mut cases = Vec::<CodegenCase>::new();
        for schema_path in codegen_dir
            .read_dir()
            .expect("rust codegen tests dir should exist")
        {
            let schema_path = schema_path.unwrap().path();
            let extension = match schema_path.extension() {
                Some(x) => x,
                None => continue,
            };
            if extension != "lavish" {
                continue;
            }

            let name = schema_path.file_stem().unwrap().to_string_lossy();
            cases.push(CodegenCase {
                name: name.into(),
                schema_path,
            });
        }
        status(format!("Found {} codegen tests", cases.len()));

        // Rust
        {
            task("Rust codegen...");

            let tmp_dir = codegen_dir.join(".tmp");
            let target_dir = tmp_dir.join("rust_target");
            let harness_dir = tmp_dir.join("rust_harness");

            if harness_dir.exists() {
                fs::remove_dir_all(&harness_dir).unwrap();
            }
            fs::create_dir_all(&harness_dir).unwrap();

            let cargo_path = harness_dir.join("Cargo.toml");
            fs::write(&cargo_path, &RUST_CODEGEN_CARGO_TEMPLATE).unwrap();

            let src_dir = harness_dir.join("src");
            fs::create_dir_all(&src_dir).unwrap();

            {
                let mut builds = Vec::<String>::new();
                for case in &cases {
                    builds.push(format!(
                        "build {name} from {path:?}",
                        name = case.name.replace("-", "_"),
                        path = case.schema_path
                    ));
                }

                let rules_path = src_dir.join("lavish-rules");
                let rules_template =
                    RUST_CODEGEN_LAVISH_RULES_TEMPLATE.replace("{{BUILDS}}", &builds.join("\n"));
                fs::write(&rules_path, &rules_template).unwrap();
            }

            Command::new(&self.compiler_path)
                .args(&["build", &src_dir.to_string_lossy()])
                .run_verbose();

            Command::new("cargo")
                .args(&["check", "--manifest-path", &cargo_path.to_string_lossy()])
                .env("CARGO_TARGET_DIR", target_dir)
                .run_verbose();
        }
    }

    fn run_compliance_tests(&self) {
        let compliance_dir = self.tests_dir.join("compliance");
        task("Compliance tests");

        let mut cases = Vec::<ComplianceCase>::new();

        {
            task("Prepare rust");
            let rust_dir = compliance_dir.join("rust_compliant");

            let src_dir = rust_dir.join("src");
            Command::new(&self.compiler_path)
                .args(&["build", &src_dir.to_string_lossy()])
                .run_verbose();

            let cargo_path = rust_dir.join("Cargo.toml");
            Command::new("cargo")
                .args(&["build", "--manifest-path", &cargo_path.to_string_lossy()])
                .run_verbose();

            let exec_path = rust_dir.join("target").join("debug").join("rust_compliant");
            cases.push(ComplianceCase {
                language: "rust".into(),
                exec: exec_path.to_string_lossy().into(),
                args: vec![],
            });
        }

        let mut pairs = Vec::<(&ComplianceCase, &ComplianceCase)>::new();

        for lhs in &cases {
            for rhs in &cases {
                pairs.push((&lhs, &rhs));
            }
        }
        status("{} pairs to test");

        for pair in &pairs {
            task(format!(
                "Compliance {}<->{}",
                pair.0.language, pair.1.language
            ));
            status(format!("L: {:#?}", pair.0));
            status(format!("R: {:#?}", pair.1));

            status("Starting server...");
            let mut server = pair.0.spawn_server();

            status("Running client");
            pair.1.run_client(&server);

            status("Killing server...");
            server.child.kill().unwrap();
        }
    }
}

pub trait RunVerbose {
    fn run_verbose(&mut self);
}

impl RunVerbose for Command {
    fn run_verbose(&mut self) {
        println!("{}", format!("{:?}", self).blue());
        let status = self.status().unwrap();
        if !status.success() {
            println!(
                "{}",
                format!("Process failed with code {:?}", status.code()).red()
            );
            std::process::exit(1);
        }
    }
}

