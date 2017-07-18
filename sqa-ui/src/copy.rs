//! Copying, cutting and pasting.
use gtk::prelude::*;
use gtk::{Window, MenuItem, TreeView, Builder};
use glib::object::ObjectExt;
use sync::UISender;
use actions::ActionInternalMessage;
#[derive(Copy, Clone, Debug)]
pub enum CopyPasteMessage {
    Copy,
    Cut,
    Paste
}
pub struct CopyPasteController {
    win: Window,
    mcopy: MenuItem,
    mcut: MenuItem,
    mpaste: MenuItem,
    act_view: TreeView,
    tx: Option<UISender>
}
impl CopyPasteController {
    pub fn new(b: &Builder, win: Window) -> Self {
        let tx = None;
        let act_view = b.get_object("sqa-ActionController-view")
            .expect("incorrect UI description, tried to get sqa-ActionController-view");
        build!(CopyPasteController using b
               with tx, win, act_view
               get mcopy, mcut, mpaste)
    }
    pub fn bind(&mut self, tx: &UISender) {
        use self::CopyPasteMessage::*;
        self.tx = Some(tx.clone());
        bind_menu_items! {
            self, tx,
            mcopy => Copy,
            mcut => Cut,
            mpaste => Paste
        };
    }
    pub fn on_message(&mut self, msg: CopyPasteMessage) {
        use self::CopyPasteMessage::*;
        if let Some(wdg) = self.win.get_focus() {
            if wdg == self.act_view {
                self.tx.as_mut().unwrap()
                    .send_internal(ActionInternalMessage::CopyPaste(msg));
                return;
            }
            match msg {
                Copy => {
                    let _ = wdg.emit("copy-clipboard", &[]);
                },
                Cut => {
                    let _ = wdg.emit("cut-clipboard", &[]);
                },
                Paste => {
                    let _ = wdg.emit("paste-clipboard", &[]);
                }
            }
        }
    }
}
