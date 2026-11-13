//! Named-pipe server implementing the closed command set from `lotus_core::ipc`.

use crate::controller::{fail, Controller};
use anyhow::Result;
use lotus_core::ipc::*;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub async fn serve(ctrl: Controller, cancel: CancellationToken) -> Result<()> {
    let sd = crate::win::acl::SecurityDescriptor::from_sddl(crate::win::acl::PIPE_SDDL)?;
    let mut server = crate::win::pipe::create_server(true, &sd)?;
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            r = server.connect() => { if let Err(e) = r { tracing::warn!("pipe connect: {e}"); } }
        }
        let connected = server;
        server = crate::win::pipe::create_server(false, &sd)?;
        let pid = crate::win::pipe::client_pid(&connected);
        tracing::debug!("client connected (pid {pid:?})");
        tokio::spawn(handle(connected, ctrl.clone(), cancel.clone()));
    }
}

async fn handle<S>(stream: S, ctrl: Controller, cancel: CancellationToken)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + 'static,
{
    let (r, mut w) = tokio::io::split(stream);
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(64);
    let writer = tokio::spawn(async move {
        while let Some(buf) = rx.recv().await {
            if w.write_all(&buf).await.is_err() {
                break;
            }
        }
    });
    let mut reader = BufReader::new(r);
    let mut line = Vec::with_capacity(4096);
    let mut status_task: Option<tokio::task::JoinHandle<()>> = None;
    loop {
        line.clear();
        let read = async {
            let mut limited = (&mut reader).take(MAX_FRAME as u64 + 1);
            limited.read_until(b'\n', &mut line).await
        };
        let n = tokio::select! {
            _ = cancel.cancelled() => break,
            n = read => n,
        };
        match n {
            Ok(0) | Err(_) => break,
            Ok(_) if line.len() > MAX_FRAME => break,
            Ok(_) => {}
        }
        let req: Request = match serde_json::from_slice(&line) {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(encode(&ServerMessage::Response { id: 0, result: None, error: Some(ErrorInfo::new("bad_request", e.to_string())) })).await;
                continue;
            }
        };
        if matches!(req.command, Command::Hello { .. }) && status_task.is_none() {
            let mut sub = ctrl.subscribe();
            let tx2 = tx.clone();
            status_task = Some(tokio::spawn(async move {
                loop {
                    let s = sub.borrow_and_update().clone();
                    if tx2.send(encode(&ServerMessage::Status { status: s })).await.is_err() {
                        break;
                    }
                    if sub.changed().await.is_err() {
                        break;
                    }
                }
            }));
        }
        let ctrl = ctrl.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            let id = req.id;
            let (result, error) = match dispatch(&ctrl, req.command).await {
                Ok(v) => (Some(v), None),
                Err(e) => (None, Some(e)),
            };
            let _ = tx.send(encode(&ServerMessage::Response { id, result, error })).await;
        });
    }
    if let Some(t) = status_task {
        t.abort();
    }
    drop(tx);
    let _ = writer.await;
}

fn to_value<T: serde::Serialize>(v: T) -> Result<Value, ErrorInfo> {
    serde_json::to_value(v).map_err(|e| fail("internal", e.to_string()))
}

async fn dispatch(ctrl: &Controller, cmd: Command) -> Result<Value, ErrorInfo> {
    match cmd {
        Command::Hello { protocol, .. } => {
            if protocol != PROTOCOL_VERSION {
                return Err(fail("version", "The LOTUS app and service versions don't match. Reinstall LOTUS."));
            }
            Ok(json!({"protocol": PROTOCOL_VERSION, "service": crate::controller::SERVICE_VERSION, "engine": lotus_core::singbox::ENGINE_VERSION}))
        }
        Command::Status => to_value(ctrl.status()),
        Command::ConnectLotus { options } => ctrl.connect_lotus(options, false).await.map(|_| Value::Null),
        Command::ConnectShadow { profile, options } => ctrl.connect_shadow(profile, options, false).await.map(|_| Value::Null),
        Command::Disconnect => {
            ctrl.stop_session(false).await;
            Ok(Value::Null)
        }
        Command::TestProfile { profile } => to_value(ctrl.test_profile(profile).await?),
        Command::SpeedTest => to_value(crate::probe::speed_test().await),
        Command::Diagnostics => to_value(crate::diagnostics::run(ctrl).await),
        Command::Logs => to_value(ctrl.inner().logs.snapshot()),
        Command::Connections => to_value(ctrl.connections().await),
        Command::Usage => to_value(ctrl.inner().usage.lock().unwrap().report()),
        Command::ClearUsage => {
            ctrl.inner().usage.lock().unwrap().clear();
            Ok(Value::Null)
        }
        Command::VerifyServer { profile } => {
            lotus_core::validate::validate_profile(&profile).map_err(|e| crate::controller::fail("invalid_profile", e.to_string()))?;
            to_value(crate::verify::run(ctrl, profile).await)
        }
    }
}
