use std::collections::HashMap;
use std::fs::File;
use std::os::fd::{AsRawFd};
use std::os::unix::prelude::PermissionsExt;
use std::process::{Command, Stdio};
use std::ptr;
use std::sync::OnceLock;
use clap::Subcommand;
use colored::Colorize;
use walkdir::WalkDir;

static BINARIES: OnceLock<HashMap<String, String>> = OnceLock::new();

fn binaries_ref() -> &'static HashMap<String, String> {
    BINARIES.get_or_init(|| {
        let mut binaries = HashMap::new();
        let dir = ".";
        for entry in WalkDir::new(dir) {
            let entry = entry.expect("Failed to read dir entry");
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();

            if let Ok(meta) = path.metadata() {
                let mode = meta.permissions().mode();
                if mode & 0o111 != 0 {
                    binaries.insert(name, path.to_string_lossy().to_string());
                }
            }
        }
        binaries
    })
}

#[derive(Subcommand, Clone, Debug)]
pub enum BinarySub {
    List,
    Exec {
        name: String,
        args: Vec<String>,
    },
    Read {
        addr: String,
    },
}

pub fn handle_binary_command(sub: &BinarySub) -> anyhow::Result<()> {
    let binaries = binaries_ref();
    match sub {
        BinarySub::List => {
            binaries
                .iter()
                .for_each(|(name, path)| println!("{} {path}", format!("{name}").bright_red()));
        }
        BinarySub::Exec { name, args } => {
            match binaries.get(name) {
                Some(path) => {
                    let mut cmd = Command::new("bash")
                        .arg(path)
                        .args(args)
                        .stdin(Stdio::inherit())
                        .stdout(Stdio::inherit())
                        .spawn()?;
                    cmd.wait()?;
                }
                None => {
                    eprintln!("Failed to find binary: {name}")
                }
            }
        }
        BinarySub::Read { addr } => read(addr)?,
    }
    Ok(())
}

pub fn read(addr: &str) -> anyhow::Result<()> {
    let offset_str = addr.strip_prefix("0x").expect("A hex number must start with `0x`");
    let offset = u64::from_str_radix(offset_str.trim(), 16).expect("Failed to parse hex number");

    let mem = File::open("/dev/mem")?;
    let fd = mem.as_raw_fd();

    let memory;
    unsafe {
        memory = libc::mmap(
            ptr::null_mut(),
            4096,
            libc::PROT_READ,
            libc::MAP_SHARED,
            fd,
            offset as libc::off_t,
        )
    }

    if memory == libc::MAP_FAILED {
        eprintln!("Failed to map memory: {addr}");
        return Ok(());
    }
    println!("Successfully mapped address: {addr}");

    // This may get a SIGBUS(7).
    let slice = unsafe { std::slice::from_raw_parts(memory as *mut u64, 64) };
    println!("The value of `{addr}`: {}", slice[0]);
    Ok(())
}

