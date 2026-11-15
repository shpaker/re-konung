//! One-request client for the viewer's optional loopback session.
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

pub fn run(session: &Path, request: &str) -> ExitCode {
    match send(session, request) {
        Ok(response) => {
            println!("{response}");
            if response["success"] == true {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("control: {error}");
            ExitCode::FAILURE
        }
    }
}

fn send(session: &Path, request: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let session: Value = serde_json::from_slice(&std::fs::read(session)?)?;
    let address: SocketAddr = session["address"]
        .as_str()
        .ok_or("Missing session address")?
        .parse()?;
    if !address.ip().is_loopback() {
        return Err("Session address must be loopback".into());
    }
    let token = session["token"].as_str().ok_or("Missing session token")?;
    let action: Value = serde_json::from_str(request)?;
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(5))?;
    stream.set_read_timeout(Some(Duration::from_secs(60)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let envelope = json!({"id":1,"token":token,"action":action});
    writeln!(stream, "{envelope}")?;
    let mut line = String::new();
    BufReader::new(stream)
        .take(1024 * 1024)
        .read_line(&mut line)?;
    Ok(serde_json::from_str(&line)?)
}
