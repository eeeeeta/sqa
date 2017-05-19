use gtk::prelude::*;
use gtk::{Statusbar, Builder, Button, Popover, ListBox, ButtonsType, DialogFlags, MessageType, MessageDialog, Window, Label};
use sync::UISender;

pub enum Message {
    Statusbar(String),
    Error(String),
    ErrorRemoved,
    ShowErrors(bool)
}
pub struct MessageController {
    ctxid: u32,
    messages: Vec<(u32, String)>,
    list: ListBox,
    sbtn: Button,
    popover: Popover,
    cbtn: Button,
    popover_lbl: Label,
    statusbar: Statusbar
}
impl MessageController {
    pub fn new(b: &Builder) -> Self {
        let ctxid = 0;
        let messages = vec![];
        let mut ctx = build!(MessageController using b
                             with ctxid, messages
                             get list, sbtn, popover, cbtn, statusbar, popover_lbl);
        ctx.ctxid = ctx.statusbar.get_context_id("default");
        ctx.statusbar.push(ctx.ctxid, "Welcome to SQA!");
        ctx
    }
    pub fn bind(&mut self, tx: &UISender) {
        self.sbtn.connect_clicked(clone!(tx; |_| {
            tx.send_internal(Message::ShowErrors(true));
        }));
        self.cbtn.connect_clicked(clone!(tx; |_| {
            tx.send_internal(Message::ShowErrors(false));
        }));
        self.list.connect_row_activated(clone!(tx; |_, row| {
            row.destroy();
            tx.send_internal(Message::ErrorRemoved);
        }));
        self.update_count();
    }
    pub fn update_count(&mut self) {
        let children = self.list.get_children().len();
        let st = format!("{} warning(s)", children);
        self.sbtn.set_label(&st);
        self.popover_lbl.set_markup(&st);
        if children == 0 {
            self.popover.hide();
        }
    }
    pub fn on_message(&mut self, msg: Message) {
        use self::Message::*;
        match msg {
            Statusbar(st) => {
                println!("statusbar: {}", st);
                let id = self.statusbar.push(self.ctxid, &st);
                self.messages.push((id, st));
            },
            Error(st) => {
                println!("warning: {}", st);
                let lbl = Label::new(None);
                lbl.set_markup(&st);
                self.list.add(&lbl);
                self.update_count();
                self.popover.show_all();
            },
            ErrorRemoved => {
                self.update_count();
            },
            ShowErrors(show) => {
                if show {
                    self.popover.show_all();
                }
                else {
                    self.popover.hide();
                }
            },
        }
    }
}
