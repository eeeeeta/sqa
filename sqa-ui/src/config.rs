//! Managing application configuration.
use gtk::prelude::*;
use gtk::{Builder, MenuItem, Entry};
use std::path::PathBuf;
use std::net::SocketAddr;
use sync::UISender;
use std::io::prelude::*;
use std::io;
use std::fs::File;
use messages::Message;
use connection::ConnectionUIMessage;
use widgets::{PropertyWindow, FallibleEntry};

pub enum ConfigMessage {
    ShowPrefs,
    SetServer(Option<SocketAddr>),
    SetServerCurrent(bool),
    NewlyConnected(SocketAddr)
}
#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Config {
    pub server: Option<SocketAddr>
}
pub struct ConfigController {
    path: PathBuf,
    pub config: Config,
    tx: Option<UISender>,
    pwin: PropertyWindow,
    ipe: FallibleEntry,
    mprefs: MenuItem,
    cur_serv: Option<SocketAddr>,
}
impl ConfigController {
    pub fn new(b: &Builder, path: PathBuf) -> Self {
        let mut pwin = PropertyWindow::new("Preferences");
        let ipe = FallibleEntry::new();
        let cfgpath = Entry::new();
        cfgpath.set_text(&path.to_string_lossy());
        cfgpath.set_sensitive(false);
        pwin.append_property("Config path", &cfgpath);
        pwin.append_property("Default server", &*ipe);
        pwin.append_close_btn();
        pwin.update_header(
            "gtk-preferences",
            "Preferences",
            ""
        );
        let mut ret = build!(ConfigController using b
                             with path, pwin, ipe
                             default config, tx, cur_serv
                             get mprefs);
        if let Err(e) = ret.load() {
            panic!("loading/creating config file failed: {:?}", e);
        }
        ret
    }
    pub fn bind(&mut self, tx: &UISender) {
        use self::ConfigMessage::*;
        self.tx = Some(tx.clone());
        bind_menu_items! {
            self, tx,
            mprefs => ShowPrefs
        };
        self.ipe.on_text_updated(clone!(tx; |slf, txt, _| {
            match txt {
                None => tx.send_internal(ConfigMessage::SetServer(None)),
                Some(addr) => {
                    match addr.parse() {
                        Ok(addr) => tx.send_internal(ConfigMessage::SetServer(Some(addr))),
                        Err(e) => slf.throw_error(&format!("{}", e))
                    }
                }
            }
        }));
        self.update();
    }
    pub fn update(&mut self) {
        self.ipe.set_text(&self.config.server.map(|x| x.to_string()).unwrap_or_default());
        self.tx.as_mut().unwrap()
            .send_internal(ConnectionUIMessage::StartupButton(self.config.server == self.cur_serv));
    }
    fn _save(&mut self) -> io::Result<()> {
        info!("saving config file: {}", self.path.to_string_lossy());
        let mut file = File::create(&self.path)?;
        let data = ::toml::ser::to_vec(&self.config).unwrap();
        file.write_all(&data)?;
        Ok(())
    }
    pub fn save(&mut self) {
        if let Err(e) = self._save() {
            self.tx.as_mut().unwrap()
                .send_internal(Message::Error(format!("failed to save config: {:?}", e)));
        }
        self.update();
    }
    pub fn load(&mut self) -> Result<(), Box<::std::error::Error>> {
        info!("loading config file: {}", self.path.to_string_lossy());
        let file = File::open(&self.path);
        match file {
            Ok(mut file) => {
                let mut contents = String::new();
                file.read_to_string(&mut contents)?;
                let config: Config = ::toml::from_str(&contents)?;
                self.config = config;
                info!("loaded config successfully");
                Ok(())
            },
            Err(e) => {
                if e.kind() == io::ErrorKind::NotFound {
                    info!("file not found, attempting to create...");
                    self._save()?;
                    info!("config file created successfully");
                    Ok(())
                }
                else {
                    Err(e.into())
                }
            }
        }
    }
    pub fn on_message(&mut self, msg: ConfigMessage) {
        use self::ConfigMessage::*;
        match msg {
            ShowPrefs => self.pwin.present(),
            SetServer(addr) => {
                self.config.server = addr;
                self.save();
            },
            SetServerCurrent(sc) => {
                if sc {
                    self.config.server = self.cur_serv.clone();
                }
                else {
                    self.config.server = None;
                }
                self.save();
            },
            NewlyConnected(sa) => {
                self.cur_serv = Some(sa);
                self.update();
            }
        }
    }
}
