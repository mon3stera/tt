use log::info;
use crate::client::{exec, upload};
use crate::module::{install_module, realm_physical_address};
use clap::Subcommand;
use std::os::unix::prelude::ExitStatusExt;
use std::process::{Command, Stdio};
use crate::qemu::{manager_ref, shared_vmm_extra_args, start_confidential_vmm_if_no_exists, QemuType};

const DEFAULT_SHARED_ADDR: &str = "0000:00:03.0";

#[derive(Subcommand, Clone, Debug)]
pub enum TestSub {
    Run { index: usize, args: Vec<String> },
}

pub async fn handle_test_command(sub: &TestSub) -> anyhow::Result<()> {
    match sub {
        TestSub::Run { index, args } => {
            match *index {
                44 => test_44()?,
                52 => test_52()?,
                60 => test_60().await?,
                601 => test_601()?,
                82 => test_82().await?,
                821 => test_821().await?,
                83 => test_83().await?,
                831 => test_831().await?,
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
    let addr = realm_physical_address()?;

    let mut manager = manager_ref().lock().unwrap();
    let port = manager.spawn_auto_port(QemuType::Normal, Some(addr))?;
    upload_tt(port).await?;

    let res = exec("/test/tt test run 601", port).await?;
    if res.stdout.contains("Test 60 passed") {
        println!("Test 60 passed. Process terminated by SIGBUS(7) as expected.");
    } else {
        println!("Test 60 failed.");
    }
    manager.stop(port);
    Ok(())
}

/// The stage2 of test 60, which will be executed in guest OS.
fn test_601() -> anyhow::Result<()> {
    let pa = pa_from_shared(DEFAULT_SHARED_ADDR)?;
    let mut tt = Command::new("/test/tt")
        .args(["binary", "read", &pa])
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

// 0000:00:03:0
fn pa_from_shared(pci: &str) -> anyhow::Result<String> {
    info!("Finding shared pa.");
    let lines = std::fs::read_to_string(format!("/sys/bus/pci/devices/{pci}/resource"))?;
    let mut pa = "";

    for line in lines.lines() {
        let line = line.trim().split(" ").collect::<Vec<_>>();
        let (start_addr, end_addr) = (
            strip_radix16(line[0])?,
            strip_radix16(line[1])?,
        );
        if end_addr - start_addr + 1 == 4096 {
            pa = line[0];
        }
    }
    info!("{}", format!("Shared pa: {}", pa));
    Ok(pa.to_string())
}

async fn test_82() -> anyhow::Result<()> {
    // this address is usually using by kernel.
    let target_addr = "0xFE940000";

    let mut manager = manager_ref().lock().unwrap();
    let port = manager.spawn_auto_port(QemuType::Confidential, Some(target_addr))?;
    upload_tt(port).await?;

    let res = exec("/test/tt test run 821", port).await?;
    if res.stdout.contains("passed") {
        println!("Test 82 passed.");
    } else {
        println!("Test 82 failed");
    }
    manager.stop(port);
    Ok(())
}

async fn test_821() -> anyhow::Result<()> {
    let pa = pa_from_shared(DEFAULT_SHARED_ADDR)?;
    if read_mem_assert_signal_bus(&pa)? {
        println!("passed");
        return Ok(())
    }
    println!("failed");
    Ok(())
}

async fn test_83() -> anyhow::Result<()> {
    todo!()
}

async fn test_831() -> anyhow::Result<()> {
    todo!()
}

fn strip_radix16(num: &str) -> anyhow::Result<u64> {
    let striped = num.strip_prefix("0x").unwrap_or(num);
    Ok(u64::from_str_radix(striped, 16)?)
}

async fn upload_tt(port: u16) -> anyhow::Result<()> {
    upload("./tt", port).await?;
    exec("chmod +x /test/tt", port).await?;
    Ok(())
}

fn read_mem_assert_signal_bus(addr: &str) -> anyhow::Result<bool> {
    let mut tt = Command::new("/test/tt")
        .args(["binary", "read", addr])
        .spawn()?;

    // This child may be killed by a signal, so we cannot check its exit code.
    let status = tt.wait()?;
    if let Some(signal) = status.signal() {
        const SIGBUS: i32 = 7;
        if signal == SIGBUS {
            return Ok(true);
        }
        return Ok(false);
    }
    Ok(false)
}