use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use clap::Subcommand;
use colored::Colorize;
use walkdir::WalkDir;

static MODULES: OnceLock<HashMap<String, String>> = OnceLock::new();

fn modules_ref() -> &'static HashMap<String, String> {
    MODULES.get_or_init(|| {
        let mut modules = HashMap::new();
        let dir = ".";
        for entry in WalkDir::new(dir) {
            let entry = entry.expect("Failed to read dir entry");
            let name = entry.file_name();
            let path = entry.path();
            if let Some(ext) = path.extension() {
                let ext = ext.to_string_lossy();
                if ext == "ko" {
                    let name = name.to_string_lossy().to_string();
                    let name = name.strip_suffix(".ko").unwrap();
                    modules.insert(name.to_string(), path.to_string_lossy().into());
                }
            }
        }
        modules
    })
}

#[derive(Subcommand, Clone, Debug)]
pub enum ModuleSub {
    List,
    Install {
        name: String,
        args: Vec<String>,
    },
    Rm {
        name: String,
    },
}

pub fn handle_module_command(sub: &ModuleSub) -> anyhow::Result<()> {
    let modules = modules_ref();
    match sub {
        ModuleSub::List => {
            modules
                .iter()
                .for_each(|(name, path)| println!("{} {path}", format!("{name}").bright_red()));
        }
        ModuleSub::Install { name, args } => install_module(name, args)?,
        ModuleSub::Rm { name } => {
            let mut cmd = Command::new("rmmod")
                .arg(name)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .spawn()?;
            cmd.wait()?;
        }
    }
    Ok(())
}

pub fn install_module(name: &str, args: &[String]) -> anyhow::Result<()> {
    match modules_ref().get(name) {
        Some(path) => {
            let mut cmd = Command::new("insmod")
                .arg(path)
                .args(args)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .spawn()?;
            cmd.wait()?;
        }
        None => {
            eprintln!("Failed to find module: {name}")
        }
    }
    Ok(())
}

pub fn realm_physical_address() -> anyhow::Result<String> {
    let provider = File::open("/proc/interface/get_realm_pa")?;
    let mut reader = BufReader::new(provider);
    let mut addr = String::new();
    reader.read_line(&mut addr)?;
    Ok(addr)
}