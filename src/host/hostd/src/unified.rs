//! `lico`: the whole product in one executable. The session host, the command line, the credential
//! command and the primary client are roles of this one program, chosen by the first argument
//! (`lico host`, `lico gui`, `lico cred`, `lico licenses`, anything else is the command line) or by
//! the name the program is run under (`lico-host`, `lico-cred`, `lico-candidate-a2`).

mod accounts;
mod admin;
mod broker;
mod call_room;
mod calls;
mod capture;
mod claude;
mod cli;
mod clipboard;
mod clipwrite;
mod codex;
mod computer;
mod core;
mod create;
mod credcmd;
mod creds;
mod dbus;
mod echo;
mod folders;
mod fsview;
mod gui;
mod handoff;
mod history;
mod helper;
mod host;
mod http;
mod ident;
mod inbox_read;
mod ipc;
mod keyring;
mod leads;
mod lifecycle;
mod lines;
mod login;
mod marks;
mod mcp;
mod memory;
mod mention;
mod notify;
mod chats;
mod pasted;
mod payload;
mod pending;
mod screentools;
mod png;
mod provider_record;
mod quota;
mod registry;
mod search;
mod session;
mod ssh;
mod ssh_remote;
mod tokens;
mod tools;
mod transfer;
mod vdisplay;
mod worktree;
mod xjob;

const LICENSE: &str = include_str!("../../../../LICENSE");
const NOTICES: &str = include_str!("../NOTICES.txt");

fn main() {
    memory::init_allocator();
    let args: Vec<String> = std::env::args().collect();
    let name = std::path::Path::new(&args[0]).file_name().and_then(|n| n.to_str()).unwrap_or("lico").to_string();
    let shifted = |first: &str| -> Vec<String> { std::iter::once(first.to_string()).chain(args.iter().skip(2).cloned()).collect() };
    match (name.as_str(), args.get(1).map(String::as_str)) {
        ("lico-host", _) => host::run(args),
        ("lico-cred", _) => credcmd::run(args),
        ("lico-candidate-a2", _) => gui::run(args),
        (_, Some("host")) if args.get(2).map(String::as_str) == Some("stop") => cli::run(args), // `lico host stop` asks a running host to end
        (_, Some("host")) => host::run(shifted("lico-host")),
        (_, Some("cred")) => credcmd::run(shifted("lico-cred")),
        (_, Some("gui")) => gui::run(shifted("lico-gui")),
        (_, Some("licenses")) => print!("{LICENSE}\n{NOTICES}"),
        (_, Some("xjob")) => xjob::run(&args[2..]), // hidden: the job side of the helper process (no usage text names it)
        (_, Some("version" | "--version")) => println!("{}", ident::cli_line()),
        _ => cli::run(args),
    }
}
