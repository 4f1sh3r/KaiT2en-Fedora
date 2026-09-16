// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 André Eikmeyer <andre.eikmeyer@kait2en.org>

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::Ipv6Addr;
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
#[path = "../cli/mod.rs"]
mod cli;
use cli::{Cli, Action, request};
use t2_bridgexpc::discovery;
#[path = "../protocols/ave.rs"]
mod ave;
use ave::Service as RemoteService;

struct Manager {
    interface: String,
    host: Ipv6Addr,
    desired: HashSet<String>,
    live: HashMap<String, RemoteService>,
    sleeping: bool,
}

impl Manager {
    fn acquire(&mut self, service: String) -> Result<()> {
        self.desired.insert(service.clone());
        if !self.sleeping && !self.live.contains_key(&service) {
            let connection = RemoteService::connect(&self.interface, self.host, &service)?;
            self.live.insert(service, connection);
        }
        Ok(())
    }

    fn release(&mut self, service: &str) -> Result<()> {
        self.desired.remove(service);
        if let Some(connection) = self.live.remove(service) {
            connection.close(service)?;
        }
        Ok(())
    }

    fn close_live(&mut self) {
        for (name, connection) in self.live.drain() {
            if let Err(error) = connection.close(&name) {
                eprintln!("could not close {name}: {error:#}");
            }
        }
    }

    fn pre_suspend(&mut self) -> Result<()> {
        self.sleeping = true;
        self.close_live();
        Ok(())
    }

    fn post_resume(&mut self) -> Result<()> {
        // Rebinding removes the old netdev; udev may give the new one a different name.
        self.sleeping = true;
        self.close_live();
        let mut link_error = None;
        for attempt in 0..20 {
            let ready = (|| -> Result<()> {
                self.interface = discovery::interface(None)?;
                discovery::host(&self.interface, Some(self.host.to_string()))?;
                Ok(())
            })();
            match ready {
                Ok(()) => {
                    link_error = None;
                    break;
                }
                Err(error) => link_error = Some(error),
            }
            if attempt != 19 {
                thread::sleep(Duration::from_secs(1));
            }
        }
        if let Some(error) = link_error {
            return Err(error.context("restore T2 NCM link after resume"));
        }
        self.sleeping = false;

        let desired: Vec<_> = self.desired.iter().cloned().collect();
        let mut last_error = None;
        for attempt in 0..20 {
            let mut pending = false;
            for service in &desired {
                if self.live.contains_key(service) {
                    continue;
                }
                match RemoteService::connect(&self.interface, self.host, service) {
                    Ok(connection) => {
                        self.live.insert(service.clone(), connection);
                    }
                    Err(error) => {
                        pending = true;
                        last_error = Some(error);
                    }
                }
            }
            if !pending {
                return Ok(());
            }
            if attempt != 19 {
                thread::sleep(Duration::from_secs(1));
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("T2 reconnect failed")))
    }

    fn status(&self) -> String {
        let mut desired: Vec<_> = self.desired.iter().map(String::as_str).collect();
        desired.sort_unstable();
        let mut live: Vec<_> = self.live.keys().map(String::as_str).collect();
        live.sort_unstable();
        format!(
            "sleeping={} desired={} live={}",
            self.sleeping,
            desired.join(","),
            live.join(",")
        )
    }
}

fn serve(socket: &Path, mut manager: Manager) -> Result<()> {
    if socket.exists() {
        fs::remove_file(socket).context("remove stale control socket")?;
    }
    let listener =
        UnixListener::bind(socket).with_context(|| format!("bind {}", socket.display()))?;

    loop {
        match manager.acquire("com.apple.aveservice".to_owned()) {
            Ok(()) => break,
            Err(error) => {
                eprintln!("waiting for com.apple.aveservice: {error:#}");
                thread::sleep(Duration::from_secs(2));
            }
        }
    }

    for incoming in listener.incoming() {
        let mut stream = incoming.context("accept control connection")?;
        let mut line = String::new();
        BufReader::new(stream.try_clone()?).read_line(&mut line)?;
        let line = line.trim();
        let result = if let Some(service) = line.strip_prefix("acquire ") {
            manager.acquire(service.to_owned()).map(|_| "OK".to_owned())
        } else if let Some(service) = line.strip_prefix("release ") {
            manager.release(service).map(|_| "OK".to_owned())
        } else if line == "pre-suspend" {
            manager.pre_suspend().map(|_| "OK".to_owned())
        } else if line == "post-resume" {
            manager.post_resume().map(|_| "OK".to_owned())
        } else if line == "status" {
            Ok(manager.status())
        } else {
            Err(anyhow::anyhow!("unknown command"))
        };
        let reply = match result {
            Ok(reply) => reply,
            Err(error) => format!("ERR {error:#}"),
        };
        // A timed-out suspend caller must not terminate the service manager.
        if let Err(error) = writeln!(stream, "{reply}") {
            eprintln!("could not send control reply: {error}");
        }
    }
    Ok(())
}


fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Action::Daemon { interface, host } => {
            let (interface, host) = loop {
                let interface = match discovery::interface(interface.clone()) {
                    Ok(interface) => interface,
                    Err(error) => {
                        eprintln!("waiting for T2 CDC-NCM interface: {error:#}");
                        thread::sleep(Duration::from_secs(2));
                        continue;
                    }
                };
                match discovery::host(&interface, host.map(|host| host.to_string())) {
                    Ok(host) => break (interface, host),
                    Err(error) => {
                        eprintln!("waiting for IPv6 on {interface}: {error:#}");
                        thread::sleep(Duration::from_secs(2));
                    }
                }
            };
            eprintln!("using T2 at [{host}%{interface}]");
            serve(
                &cli.socket,
                Manager {
                    interface,
                    host,
                    desired: HashSet::new(),
                    live: HashMap::new(),
                    sleeping: false,
                },
            )
        }
        Action::Acquire { service } => request(&cli.socket, &format!("acquire {service}")),
        Action::Release { service } => request(&cli.socket, &format!("release {service}")),
        Action::PreSuspend => request(&cli.socket, "pre-suspend"),
        Action::PostResume => request(&cli.socket, "post-resume"),
        Action::Status => request(&cli.socket, "status"),
    }
}
