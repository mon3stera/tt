use std::fmt::{Display, Formatter};
use std::path::Path;
use clap::Subcommand;
use reqwest::Body;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use bytes::BytesMut;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Subcommand, Debug, Clone)]
pub enum ClientSub {
    Upload {
        src: String,
    },
    Exec {
        command: String,
    },
}

pub async fn handle_client_command(sub: &ClientSub, port: u16) -> anyhow::Result<()> {
    match sub {
        ClientSub::Upload { src } => upload(src, port).await?,
        ClientSub::Exec { command } => {
            let res = exec(command, port).await?;
            println!("{}", serde_json::to_string_pretty(&res)?);
        }
    }
    Ok(())
}

pub async fn upload(src: &str, port: u16) -> anyhow::Result<()> {
    let path = Path::new(src);
    let name = path.file_name().unwrap().to_str().unwrap();

    let mut buf = Vec::new();
    let mut file = File::open(src).await?;
    file.read_to_end(&mut buf).await?;

    let res = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/upload/{name}"))
        .body(Body::from(buf))
        .send()
        .await?;
    println!("{}", res.text().await?);
    Ok(())
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExecRes {
    success: String,
    stdout: String,
    stderr: String,
    error: Option<String>,
}

impl Display for ExecRes {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", serde_json::to_string_pretty(self).unwrap())
    }
}

pub async fn exec(command: &str, port: u16) -> anyhow::Result<ExecRes> {
    let res = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/exec"))
        .body(json!({
            "command": command,
        }).to_string())
        .send()
        .await?;
    Ok(res.json::<ExecRes>().await?)
}