use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use clap::Subcommand;
use colored::Colorize;
use log::info;

static MANAGER: OnceLock<Mutex<QemuManager>> = OnceLock::new();

fn manager_ref() -> &'static Mutex<QemuManager> {
    MANAGER.get_or_init(|| Mutex::new(QemuManager::default()))
}

#[derive(Subcommand, Debug, Clone)]
pub enum QemuSub {
    Start {
        #[clap(default_value_t = 8088)]
        port: u16,
        typ: QemuType,
        #[clap(short = 's')]
        shared: Option<String>,
    },
    Stop {
        port: u16,
    },
}

pub fn handle_qemu_command(sub: &QemuSub) -> anyhow::Result<()> {
    let mut manager = manager_ref().lock().unwrap();
    match sub {
        QemuSub::Start { port, typ, shared } => {
            manager.spawn(*port, *typ, shared.as_ref())?;
        }
        QemuSub::Stop { port } => {
            manager.stop(*port);
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub enum QemuType {
    Normal,
    Confidential,
}

impl From<String> for QemuType {
    fn from(value: String) -> Self {
        match value.as_str() {
            "normal" => QemuType::Normal,
            "confidential" => QemuType::Confidential,
            _ => QemuType::Normal,
        }
    }
}

#[derive(Default)]
pub struct QemuManager {
    instances: HashMap<u16, QemuGuard>,
}

impl QemuManager {
    pub fn spawn(
        &mut self,
        port: u16,
        typ: QemuType,
        shared: Option<impl AsRef<str>>,
    ) -> anyhow::Result<()> {
        let mut child = Command::new("qemu-system-aarch64");

        child
            .args(basic_vmm_args(port));
        if matches!(typ, QemuType::Confidential) {
            child.args(&confidential_vmm_extra_args());
        }
        if let Some(addr) = shared {
            child.args(shared_vmm_extra_args(addr.as_ref()));
        }

        let guard = QemuGuard {
            instance: child.spawn()?,
            port,
            typ,
        };
        self.instances.insert(port, guard);
        info!("{}", format!("Successfully spawned a qemu process with port {port}"));
        Ok(())
    }

    pub fn stop(&mut self, port: u16) {
        self.instances.remove(&port);
    }
}

pub struct QemuGuard {
    typ: QemuType,
    instance: Child,
    port: u16,
}

impl Drop for QemuGuard {
    fn drop(&mut self) {
        if let Err(e) = self.instance.kill() {
            info!("{}", format!("Failed to stop qemu at port {}", self.port));
        }
    }
}

pub fn basic_vmm_args(port: u16) -> Vec<String> {
    vec![
        "-nodefaults",
        "-chardev", "stdio,mux=on,id=chr0,signal=off",
        "-serial", "chardev:chr0",
        "-device", "virtio-serial-pci",
        "-device", "virtconsole,chardev=chr0",
        "-mon", "chardev=chr0,mode=readline",
        "-device", "virtio-net-pci,netdev=net0,romfile=",
        "-netdev", &format!("user,id=net0,hostfwd=tcp::{port}-:8080"),
        "-cpu", "host",
        "-M", "virt",
        "-enable-kvm",
        "-M", "gic-version=3,its=on",
        "-smp", "2",
        "-m", "1G",
        "-nographic",
        "-kernel", "/mnt/out/bin/Image",
        "-initrd", "/mnt/out-br/images/rootfs.cpio",
        "-append", "console=hvc0",
    ]
        .into_iter()
        .map(|e| e.to_string())
        .collect()
}

pub fn confidential_vmm_extra_args() -> Vec<String> {
    vec![
        "-M", "confidential-guest-support=rme0",
        "-object", "rme-guest,id=rme0,measurement-algorithm=sha512,personalization-value=ICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgIEknbSBhIHRlYXBvdA==,measurement-log=off",
        "-dtb", "/root/qemu-gen.dtb"
    ]
        .into_iter()
        .map(|e| e.to_string())
        .collect()
}

pub fn shared_vmm_extra_args(offset: &str) -> Vec<String> {
    let offset = offset.trim();
    vec![
        "-object", &format!("memory-backend-file,id=physmem,size=4K,mem-path=/dev/mem,offset={offset},share=on"),
        "-device", "ivshmem-plain,memdev=physmem,id=ivshmem0",
    ]
        .into_iter()
        .map(|e| e.to_string())
        .collect()
}

pub fn vmm_exists(port: u16) -> anyhow::Result<bool> {
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

pub async fn start_normal_vmm_if_no_exists(args: &[String], port: u16) -> anyhow::Result<Option<Child>> {
    if vmm_exists(port)? {
        return Ok(None);
    }

    let cmd = Command::new("qemu-system-aarch64")
        .args(basic_vmm_args(port))
        .args(args)
        .spawn()?;

    info!("{}", "Sleeping for 45 seconds to wait for vmm.".bright_red());
    tokio::time::sleep(Duration::from_secs(45)).await;
    Ok(Some(cmd))
}

pub async fn start_confidential_vmm_if_no_exists(args: &[String], port: u16) -> anyhow::Result<Option<Child>> {
    if vmm_exists(port)? {
        return Ok(None);
    }

    let cmd = Command::new("qemu-system-aarch64")
        .args(basic_vmm_args(port))
        .args(confidential_vmm_extra_args())
        .args(args)
        .spawn()?;

    info!("{}", "Sleeping for one minute to wait for vmm.".bright_red());
    tokio::time::sleep(Duration::from_secs(60)).await;
    Ok(Some(cmd))
}