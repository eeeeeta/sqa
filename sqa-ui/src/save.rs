use gtk::prelude::*;
use gtk::{MenuItem, FileChooserDialog, ResponseType, FileChooserAction, Window, Builder};
use sync::UISender;
use std::path::Path;
use sqa_backend::actions::audio::Controller;
use sqa_backend::codec::{Command, Reply};
use messages::Message;

pub enum SaveMessage {
    OpenDialog(bool),
    SaveDialog,
    SaveAsDialog,
    Open(String, bool),
    Save(String),
    Autosave,
    External(Reply),
    NewlyConnected
}
pub struct SaveController {
    cur_file: Option<String>,
    mopen: MenuItem,
    mopen_force: MenuItem,
    msave: MenuItem,
    msaveas: MenuItem,
    tx: Option<UISender>,
    win: Window
}
impl SaveController {
    pub fn new(b: &Builder, win: Window) -> Self {
        let (cur_file, tx) = (None, None);
        build!(SaveController using b
               with cur_file, tx, win
               get mopen, mopen_force, msave, msaveas)
    }
    pub fn bind(&mut self, tx: &UISender) {
        use self::SaveMessage::*;
        self.tx = Some(tx.clone());
        bind_menu_items! {
            self, tx,
            mopen => OpenDialog(false),
            mopen_force => OpenDialog(true),
            msave => SaveDialog,
            msaveas => SaveAsDialog
        };
        self.update();
    }
    fn make_dialog(&mut self, open: bool, force: bool) {
        let msg = if open { "Select a file to open" } else { "Select a file to save as" };
        let act = if open { FileChooserAction::Open } else { FileChooserAction::Save };
        let diag = FileChooserDialog::new(Some(msg), Some(&self.win), act);
        diag.set_modal(true);
        diag.add_button("Cancel", ResponseType::Cancel.into());
        if open {
            diag.add_button("Open", ResponseType::Accept.into());
        }
        else {
            diag.add_button("Save", ResponseType::Accept.into());
        }
        let tx = self.tx.as_ref().unwrap().clone();
        diag.connect_response(move |slf, resp| {
            // this match statement is sloppy because of gtk-rs' lack of
            // `From<i32>` impl for ResponseType (or similar)
            match resp {
                _x if _x == ResponseType::Accept.into() => {
                    if let Some(url) = slf.get_uri() {
                        let msg;
                        if open {
                            msg = SaveMessage::Open(url, force);
                        }
                        else {
                            msg = SaveMessage::Save(url);
                        }
                        tx.send_internal(msg);
                        slf.destroy();
                    }
                },
                _x if _x == ResponseType::Cancel.into() => slf.destroy(),
                _x if _x == ResponseType::DeleteEvent.into() => {},
                x => warn!("odd dialog response type: {}", x)
            }
        });
        diag.connect_file_activated(move |slf| {
            slf.response(ResponseType::Accept.into());
        });
        diag.show_all();
    }
    fn update(&mut self) {
        self.win.set_title(&format!("SQA ({}): no file", ::sqa_backend::VERSION));
        if let Some(cur_file) = self.cur_file.as_ref() {
            let pth = Path::new(cur_file);
            if let Some(f) = pth.file_name() {
                let f = f.to_string_lossy();
                self.msave.set_label(&format!("Save {}", f));
                self.win.set_title(&format!("SQA ({}): {}", ::sqa_backend::VERSION, f));
            }
            else {
                self.win.set_title(&format!("SQA ({}): file opened", ::sqa_backend::VERSION));
                self.msave.set_label("Save current file");
            }
        }
        else {
            self.msave.set_label("Save...");
        }
    }
    pub fn parse_uri(&mut self, uri: &str) -> Option<String> {
        match Controller::parse_url(uri) {
            Ok(u) => Some(u.to_string_lossy().into()),
            Err(e) => {
                self.tx.as_mut().unwrap()
                    .send_internal(Message::Error(format!("Error parsing filename: {}", e)));
                error!("Error parsing FileChooser URI: {}", e);
                None
            }
        }
    }
    pub fn on_message(&mut self, msg: SaveMessage) {
        use self::SaveMessage::*;
        match msg {
            OpenDialog(force) => self.make_dialog(true, force),
            Autosave | SaveDialog if self.cur_file.is_some() => {
                self.tx.as_mut().unwrap()
                    .send(Command::MakeSavefile {
                        save_to: self.cur_file.as_ref().unwrap().clone()
                    });
            },
            SaveDialog | SaveAsDialog => self.make_dialog(false, false),
            Open(uri, force) => {
                if let Some(uri) = self.parse_uri(&uri) {
                    self.cur_file = Some(uri.clone());
                    self.tx.as_mut().unwrap()
                        .send(Command::LoadSavefile {
                            load_from: uri,
                            force
                        });
                }
            },
            Save(uri) => {
                if let Some(uri) = self.parse_uri(&uri) {
                    self.cur_file = Some(uri.clone());
                    self.tx.as_mut().unwrap()
                        .send(Command::MakeSavefile {
                            save_to: uri
                        });
                }
            },
            External(rpl) => {
                use self::Reply::*;
                match rpl {
                    SavefileLoaded { res } => {
                        if !action_reply_notify!(self, res, "Loading savefile", "Savefile loaded.") {
                            self.cur_file = None;
                        }
                    },
                    SavefileMade { res } => {
                        let pth = format!("Saved to {}.",
                                          self.cur_file.as_ref().map(|x| x as &str).unwrap_or("???"));
                        if !action_reply_notify!(self, res, "Saving", pth) {
                            self.cur_file = None;
                        }
                    },
                    x => warn!("unexpected reply {:?}", x)
                }
            },
            Autosave => {},
            NewlyConnected => {
                self.cur_file = None;
            }
        }
        self.update();
    }
}
