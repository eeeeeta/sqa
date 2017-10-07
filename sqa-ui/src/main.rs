extern crate gtk;
extern crate sqa_backend;
extern crate rosc;
extern crate tokio_core;
extern crate futures;
extern crate glib;
extern crate time;
#[macro_use] extern crate error_chain;
extern crate gdk;
extern crate uuid;
extern crate url;
#[macro_use] extern crate log;
extern crate fern;
extern crate clap;
extern crate app_dirs;
extern crate serde;
#[macro_use] extern crate serde_derive;
extern crate cairo;
extern crate toml;
#[macro_use] extern crate derive_is_enum_variant;

use gtk::prelude::*;
use gtk::{Builder, Window};
use std::thread;
use tokio_core::reactor::Core;
use futures::sync::mpsc;
use std::sync::mpsc as smpsc;
use std::cell::RefCell;
use std::net::SocketAddr;
use clap::{Arg, App};
use app_dirs::{AppDataType, AppInfo};

const APP_INFO: AppInfo = AppInfo { name: "SQA", author: "eta" };

#[macro_use]
mod util;
mod widgets;
mod errors;
mod sync;
mod messages;
mod actions;
mod connection;
mod save;
mod copy;
mod config;

use sync::{UIContext, BackendContext};
fn main() {
    let matches = App::new("SQA UI")
        .version(sqa_backend::VERSION)
        .author("eta <http://theta.eu.org>")
        .about("GTK+ frontend for SQA, an application for live audio")
        .arg(Arg::with_name("config")
             .short("c")
             .long("config")
             .value_name("FILE")
             .help("Path to a custom configuration file.")
             .takes_value(true))
        .arg(Arg::with_name("server")
             .short("s")
             .long("server")
             .value_name("IP:PORT")
             .help("Connect to this server on startup (overrides server specified in config file).")
             .takes_value(true))
        .arg(Arg::with_name("v")
             .short("v")
             .multiple(true)
             .help("Verbosity: default = INFO, -v = DEBUG, -vv = TRACE"))
        .get_matches();
    let config = matches.value_of("config").map(|x| x.into()).unwrap_or_else(|| {
        let mut path = app_dirs::app_root(AppDataType::UserConfig, &APP_INFO).unwrap();
        path.push("ui_config.toml");
        path
    });
    let mut server: Option<SocketAddr> = matches.value_of("server").map(|x| {
        x.parse().expect("failed to parse server argument, ensure it's in the format IP:PORT")
    });
    let loglevel = match matches.occurrences_of("v") {
        0 => log::LogLevelFilter::Info,
        1 => log::LogLevelFilter::Debug,
        _ => log::LogLevelFilter::Trace
    };
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!("[{}] {} {}", record.target(), record.level(), message))
        })
        .level(loglevel)
        .level_for("tokio_core", log::LogLevelFilter::Info)
        .level_for("mio", log::LogLevelFilter::Info)
        .chain(::std::io::stdout())
        .apply()
        .unwrap();
    info!("SQA UI, using version {}", sqa_backend::VERSION);
    info!("an eta project <http://theta.eu.org>");
    info!("[*] Using config file: {}", config.to_string_lossy());
    info!("[+] Initialising GTK+");
    let _ = gtk::init().unwrap();
    let b = Builder::new_from_string(util::INTERFACE_SRC);
    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("ui.css")).unwrap();
    let screen = gdk::Screen::get_default().unwrap();
    gtk::StyleContext::add_provider_for_screen(&screen, &provider, gtk::STYLE_PROVIDER_PRIORITY_USER);
    glib::set_application_name("SQA");
    glib::set_prgname(Some("SQA"));
    info!("[+] Reading configuration");
    let cc = config::ConfigController::new(&b, config);
    if server.is_none() && cc.config.server.is_some() {
        server = cc.config.server.clone();
    }
    info!("[+] Initialising event loop & backend context");
    let tn = util::ThreadNotifier::new();
    let ttn = tn.clone();
    let (btx, brx) = mpsc::unbounded();
    let (utx, urx) = smpsc::channel();
    let tutx = utx.clone();
    let btx2 = btx.clone();
    let win: Window = b.get_object("sqa-main").unwrap();
    thread::spawn(move || {
        let mut core = Core::new().unwrap();
        let mut ctx = BackendContext {
            conn: connection::Context::new(),
            tn: ttn,
            rx: brx,
            tx: tutx,
            hdl: core.handle()
        };
        core.run(&mut ctx).unwrap();
        panic!("The future resolved! What is this sorcery?!");
    });
    info!("[+] Initialising UI context");
    let mut ctx = UIContext {
        rx: urx,
        tx: btx,
        stn: tn.clone(),
        stx: utx,
        conn: connection::ConnectionController::new(&b, win.clone()),
        act: actions::ActionController::new(&b),
        msg: messages::MessageController::new(&b),
        save: save::SaveController::new(&b, win.clone()),
        copy: copy::CopyPasteController::new(&b, win.clone()),
        config: cc
    };
    ctx.bind_all();
    if let Some(srv) = server {
        btx2.send(connection::ConnectionMessage::Connect(srv).into()).unwrap();
        ctx.on_event();
    }
    let ctx = RefCell::new(ctx);
    tn.register_handler(move || {
        ctx.borrow_mut().on_event();
    });
    info!("[+] Showing main window");
    win.present();
    win.connect_delete_event(|_, _| {
        gtk::main_quit();
        Inhibit(false)
    });
    info!("[+] Starting GTK+ event loop!");
    gtk::main();
}
