use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use clap::Subcommand;
use colored::Colorize;
use walkdir::WalkDir;

static SCRIPTS: OnceLock<HashMap<String, String>> = OnceLock::new();

fn script_ref() -> &'static HashMap<String, String> {
    SCRIPTS.get_or_init(|| {
        let mut scripts = HashMap::new();
        let dir = ".";
        for entry in WalkDir::new(dir) {
            let entry = entry.expect("Failed to read dir entry");
            let name = entry.file_name();
            let path = entry.path();
            if let Some(ext) = path.extension() {
                let ext = ext.to_string_lossy();
                if ext == "sh" {
                    let name = name.to_string_lossy().to_string();
                    let name = name.strip_suffix(".sh").unwrap();
                    scripts.insert(name.to_string(), path.to_string_lossy().into());
                }
            }
        }
        scripts
    })
}

#[derive(Subcommand, Clone, Debug)]
pub enum ScriptSub {
    List,
    Exec { name: String },
}

pub fn handle_script_command(sub: &ScriptSub) -> anyhow::Result<()> {
    let scripts = script_ref();
    match sub {
        ScriptSub::List => {
            scripts
                .iter()
                .for_each(|(name, path)| println!("{} {path}", format!("{name}").bright_red()));
        }
        ScriptSub::Exec { name } => {
            match scripts.get(name) {
                Some(path) => {
                    let mut cmd = Command::new("bash")
                        .arg(path)
                        .stdin(Stdio::inherit())
                        .stdout(Stdio::inherit())
                        .spawn()?;
                    cmd.wait()?;
                }
                None => {
                    eprintln!("Failed to find script: {name}")
                }
            }
        }
    }
    Ok(())
}