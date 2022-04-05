use semver::Version;
use std::fs::File;
use std::io::{self, Read, Write};
use std::io::{Error, ErrorKind};
use std::io::{Seek, SeekFrom};
use std::net::Ipv4Addr;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::result::Result;
use std::str;
use std::thread;
use std::time::Instant;
use structopt::StructOpt;

mod client;
mod crypto;
mod server;
mod teleport;
mod utils;

/// Teleporter is a simple application for sending files from Point A to Point B

#[derive(Clone, Debug, StructOpt)]
pub struct Opt {
    /// List of filepaths to files that will be teleported
    #[structopt(short, long, parse(from_os_str), default_value = "")]
    input: Vec<PathBuf>,

    /// Destination teleporter IP address
    #[structopt(short, long, default_value = "127.0.0.1")]
    dest: String,

    /// Destination teleporter Port, or Port to listen on
    #[structopt(short, long, default_value = "9001")]
    port: u16,

    #[structopt(long, env = "RNDZ_SERVER")]
    rndz_server: Option<String>,

    #[structopt(long, env = "LOCAL_ID")]
    local_id: Option<String>,

    #[structopt(long, env = "REMOTE_ID")]
    remote_id: Option<String>,

    /// Overwrite remote file
    #[structopt(short, long)]
    overwrite: bool,

    /// Recurse into directories on send
    #[structopt(short, long)]
    recursive: bool,

    /// Encrypt the file transfer using ECDH key-exchange and random keys
    #[structopt(short, long)]
    encrypt: bool,

    /// Disable delta transfer (overwrite will transfer entire file)
    #[structopt(short, long)]
    no_delta: bool,

    /// Keep path info (recreate directory path on remote server)
    #[structopt(short, long)]
    keep_path: bool,

    /// Allow absolute and relative file paths for transfers (server only) [WARNING: potentially dangerous option, use at your own risk!]
    #[structopt(long)]
    allow_dangerous_filepath: bool,

    /// Backup the destination file to a ".bak" extension if it exists and is being overwritten (consecutive runs will replace the *.bak file)
    #[structopt(short, long)]
    backup: bool,

    /// If the destination file exists, append a ".1" (or next available number) to the filename instead of overwriting
    #[structopt(short, long)]
    filename_append: bool,

    /// Require encryption for incoming connections to the server
    #[structopt(short, long)]
    must_encrypt: bool,
}

const PROTOCOL: u64 = 0x54524f50454c4554;
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    // Process arguments
    let opt = Opt::from_args();
    let out;

    // If the input filepath list is empty, assume we're in server mode
    if opt.input.len() == 1 && opt.input[0].to_str().unwrap() == "" {
        out = server::run(opt);
    // Else, we have files to send so we're in client mode
    } else {
        out = client::run(opt);
    }
    match out {
        Ok(()) => {}
        Err(s) => println!("Error: {}", s),
    };
}
