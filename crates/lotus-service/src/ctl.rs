//! `lotus-service ctl …` — a command-line client for the running service.
//! Useful for scripting and for verifying the service without the desktop app.

use anyhow::{bail, Context, Result};
use lotus_core::import::parse_import;
use lotus_core::ipc::*;
use lotus_core::*;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const USAGE: &str = "usage: lotus-service ctl <command>
  status                         print current status
  watch                          stream status changes
  lotus [--provider P] [--transport doh|doh3|dot] [--no-trackers] [--malware] [--allow-plain-dns]
  shadow <link | @file> [--no-kill-switch] [--no-lan] [--no-reconnect] [--ipv6-off] [--trackers]
  test <link | @file>            test a server without changing routing
  disconnect
  speed                          run a speed test over the current connection
  diag                           run leak diagnostics
  logs                           recent engine/service messages
  connections                    live connections through the running session
  verify <link | @file>          check the server: authenticated handshake, what strangers see, exit
  usage                          local usage history (time and traffic per day)";

fn load_profile(arg: &str) -> Result<ServerProfile> {
    let text = if let Some(path) = arg.strip_prefix('@') { std::fs::read_to_string(path).with_context(|| format!("read {path}"))? } else { arg.to_string() };
    let mut r = parse_import(&text);
    if r.profiles.is_empty() {
        bail!("{}", r.issues.first().map(|i| i.message.clone()).unwrap_or_else(|| "no profile found".into()));
    }
    Ok(r.profiles.remove(0))
}

fn flag(args: &[String], f: &str) -> bool {
    args.iter().any(|a| a == f)
}

fn opt<'a>(args: &'a [String], f: &str) -> Option<&'a str> {
    args.iter().position(|a| a == f).and_then(|i| args.get(i + 1)).map(|s| s.as_str())
}

pub fn main(args: &[String]) -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    rt.block_on(run(args))
}

async fn run(args: &[String]) -> Result<()> {
    let Some(cmd) = args.first() else {
        println!("{USAGE}");
        return Ok(());
    };
    let rest = &args[1..];
    let command = match cmd.as_str() {
        "status" | "watch" => Command::Status,
        "disconnect" => Command::Disconnect,
        "speed" => Command::SpeedTest,
        "diag" => Command::Diagnostics,
        "logs" => Command::Logs,
        "connections" => Command::Connections,
        "usage" => Command::Usage,
        "verify" => Command::VerifyServer { profile: load_profile(rest.first().context("missing link or @file")?)? },
        "lotus" => {
            let mut o = LotusOptions::default();
            if let Some(p) = opt(rest, "--provider") {
                o.provider = serde_json::from_value(Value::String(p.into())).context("unknown provider")?;
            }
            if let Some(t) = opt(rest, "--transport") {
                o.transport = serde_json::from_value(Value::String(t.into())).context("unknown transport")?;
            }
            o.block_trackers = !flag(rest, "--no-trackers");
            o.block_malware = flag(rest, "--malware");
            o.block_plain_dns = !flag(rest, "--allow-plain-dns");
            Command::ConnectLotus { options: o }
        }
        "shadow" | "test" => {
            let target = rest.first().context("missing link or @file")?;
            let profile = load_profile(target)?;
            if cmd == "test" {
                Command::TestProfile { profile }
            } else {
                let o = ShadowOptions {
                    kill_switch: !flag(rest, "--no-kill-switch"),
                    allow_lan: !flag(rest, "--no-lan"),
                    auto_reconnect: !flag(rest, "--no-reconnect"),
                    ipv6: if flag(rest, "--ipv6-off") { Ipv6Mode::Disable } else { Ipv6Mode::Tunnel },
                    block_trackers: flag(rest, "--trackers"),
                    ..Default::default()
                };
                Command::ConnectShadow { profile, options: o }
            }
        }
        _ => {
            println!("{USAGE}");
            return Ok(());
        }
    };

    let pipe = crate::win::pipe::open_client().context("connect to the LOTUS service (is it installed and running?)")?;
    let (r, mut w) = tokio::io::split(pipe);
    let mut lines = BufReader::new(r).lines();
    w.write_all(&encode(&Request { id: 1, command: Command::Hello { protocol: PROTOCOL_VERSION, client: "lotus-ctl".into() } })).await?;
    w.write_all(&encode(&Request { id: 2, command })).await?;
    let watch = cmd == "watch";
    while let Some(line) = lines.next_line().await? {
        let msg: ServerMessage = serde_json::from_str(&line)?;
        match msg {
            ServerMessage::Status { status } if watch => println!("{}", serde_json::to_string(&status)?),
            ServerMessage::Response { id: 1, error: Some(e), .. } => bail!("{}", e.message),
            ServerMessage::Response { id: 2, result, error } => {
                if let Some(e) = error {
                    bail!("{} ({})", e.message, e.code);
                }
                if !watch {
                    println!("{}", serde_json::to_string_pretty(&result.unwrap_or(Value::Null))?);
                    return Ok(());
                }
            }
            _ => {}
        }
    }
    Ok(())
}
