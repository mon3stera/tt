use crate::client::upload;
use crate::module::{install_module, realm_physical_address};
use clap::Subcommand;
use colored::Colorize;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::os::unix::prelude::ExitStatusExt;
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;
use walkdir::WalkDir;

static TESTS: OnceLock<HashMap<usize, String>> = OnceLock::new();

fn tests_ref() -> &'static HashMap<usize, String> {
    TESTS.get_or_init(|| {
        let mut tests = HashMap::new();
        let dir = ".";
        for entry in WalkDir::new(dir) {
            let entry = entry.expect("Failed to read dir entry");
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();
            if name == "test.sh" {
                let parent = path.parent().unwrap().to_string_lossy();
                let (_, parent) = parent.split_once('/').unwrap();
                let index = parent.parse::<usize>().unwrap();
                tests.insert(index, path.to_string_lossy().to_string());
            }
        }
        tests
    })
}

#[derive(Subcommand, Clone, Debug)]
pub enum TestSub {
    List,
    Exec { name: usize },
    Run { index: usize, args: Vec<String> },
}

pub async fn handle_test_command(sub: &TestSub) -> anyhow::Result<()> {
    let tests = tests_ref();
    match sub {
        TestSub::List => {
            tests
                .iter()
                .for_each(|(name, path)| println!("{} {path}", format!("{name}").bright_red()));
        }
        TestSub::Exec { name } => {
            match tests.get(name) {
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
        TestSub::Run { index, args } => {
            match *index {
                44 => test_44()?,
                52 => test_52()?,
                60 => test_60().await?,
                601 => test_601()?,
                _ => todo!(),
            }
        }
    }
    Ok(())
}

fn test_44() -> anyhow::Result<()> {
    install_module("realm_pa_provider", &[])?;

    let addr = realm_physical_address()?;
    let mut tt = Command::new("./tt")
        .args(["binary", "read", &addr])
        .spawn()?;

    // This child may be killed by a signal, so we cannot check its exit code.
    let status = tt.wait()?;
    if let Some(signal) = status.signal() {
        const SIGBUS: i32 = 7;
        if signal == SIGBUS {
            println!("Test 44 passed: Process terminated by SIGBUS(7) as expected.");
            return Ok(());
        }
        eprintln!(
            "Test 44 failed: Process terminated by an unexpected signal: {}",
            signal
        );
        return Ok(());
    }
    eprintln!(
        "Test 44 failed: Process exited normally with code {:?} but expected a Bus error signal.",
        status.code()
    );
    Ok(())
}

fn test_52() -> anyhow::Result<()> {
    install_module("test_52", &[])?;

    if std::fs::read_to_string("/proc/tee-tests/52/result")?.trim() == "ok" {
        println!("Test 52 passed.");
        return Ok(());
    }
    eprintln!("Test 52 failed.");
    Ok(())
}

/// The stage1 of test 60, which will be executed in host OS.
async fn test_60() -> anyhow::Result<()> {
    install_module("realm_pa_provider", &[])?;
    upload("./tt", 8088).await?;
    Ok(())
}

/// The stage2 of test 60, which will be executed in guest OS.
fn test_601() -> anyhow::Result<()> {
    let lines = std::fs::read_to_string("/sys/bus/pci/devices/0000:00:03:0/resource")?;
    let mut pa = "";

    for line in lines.lines() {
        let line = line.trim().split(" ").collect::<Vec<_>>();
        let (start_addr, end_addr) = (
            u64::from_str_radix(line[0], 16)?,
            u64::from_str_radix(line[1], 16)?,
        );
        if end_addr - start_addr + 1 == 4096 {
            pa = line[0];
        }
    }

    let mut tt = Command::new("./tt")
        .args(["binary", "read", pa])
        .spawn()?;

    // This child may be killed by a signal, so we cannot check its exit code.
    let status = tt.wait()?;
    if let Some(signal) = status.signal() {
        const SIGBUS: i32 = 7;
        if signal == SIGBUS {
            println!("Test 60 passed: Process terminated by SIGBUS(7) as expected.");
            return Ok(());
        }
        eprintln!(
            "Test 60 failed: Process terminated by an unexpected signal: {}",
            signal
        );
        return Ok(());
    }
    eprintln!(
        "Test 60 failed: Process exited normally with code {:?} but expected a Bus error signal.",
        status.code()
    );
    Ok(())
}

fn basic_vmm_args(port: u16) -> Vec<String> {
    vec![
        "-nodefaults",
        "-chardev stdio,mux=on,id=chr0,signal=off",
        "-serial chardev:chr0",
        "-device virtio-serial-pci",
        "-device virtconsole,chardev=chr0",
        "-mon chardev=chr0,mode=readline",
        "-device virtio-net-pci,netdev=net0,romfile=",
        &format!("-netdev user,id=net0,hostfwd=tcp::{port}-:8080"),
        "-cpu host",
        "-M virt",
        "-enable-kvm",
        "-M gic-version=3,its=on",
        "-smp 2",
        "-m 512M",
        "-nographic",
        "-kernel /mnt/out/bin/Image",
        "-initrd /mnt/out-br/images/rootfs.cpio",
        "-append console=hvc0",
    ]
        .into_iter()
        .map(|e| e.to_string())
        .collect()
}

fn confidential_vmm_extra_args() -> Vec<String> {
    vec![
        "-M confidential-guest-support=rme0",
        "-object rme-guest,id=rme0,measurement-algorithm=sha512,personalization-value=ICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgIEknbSBhIHRlYXBvdA==,measurement-log=off",
        "-dtb /root/qemu-gen.dtb"
    ]
        .into_iter()
        .map(|e| e.to_string())
        .collect()
}

fn shared_vmm_extra_args(offset: &str) -> Vec<String> {
    vec![
        &format!("-object memory-backend-file,id=physmem,size=4K,mem-path=/dev/mem,offset={offset},share=on"),
        "-device ivshmem-plain,memdev=physmem,id=ivshmem0",
    ]
        .into_iter()
        .map(|e| e.to_string())
        .collect()
}

fn vmm_exists(port: u16) -> anyhow::Result<bool> {
    let ps = Command::new("ps")
        .arg("aux")
        .stdout(Stdio::piped())
        .spawn()?;
    let stdout = ps.stdout.expect("Failed to get stdout of ps");

    let grep = Command::new("grep")
        .arg("[q]emu-system-aarch64")
        .stdin(stdout)
        .stdout(Stdio::piped())
        .spawn()?;
    let stdout = grep.stdout.expect("Failed to get stdout of grep1");

    let mut grep = Command::new("grep")
        .arg(format!("hostfwd=tcp::{port}-:8080"))
        .stdin(stdout)
        .spawn()?;
    let code = grep.wait()?.code().expect("Failed to get the exit code of grep");
    Ok(code == 0)
}

async fn start_normal_vmm_if_no_exists(args: &[String], port: u16) -> anyhow::Result<Option<Child>> {
    if vmm_exists(port)? {
        return Ok(None);
    }

    let cmd = Command::new("qemu-system-aarch64")
        .args(basic_vmm_args(port))
        .args(args)
        .spawn()?;
    // Sleep for a minute to wait for vmm.
    tokio::time::sleep(Duration::from_secs(60)).await;
    Ok(Some(cmd))
}

async fn start_confidential_vmm_if_no_exists(args: &[String], port: u16) -> anyhow::Result<Option<Child>> {
    if vmm_exists(port)? {
        return Ok(None);
    }

    let cmd = Command::new("qemu-system-aarch64")
        .args(basic_vmm_args(port))
        .args(confidential_vmm_extra_args())
        .args(args)
        .spawn()?;
    tokio::time::sleep(Duration::from_secs(60)).await;
    Ok(Some(cmd))
}