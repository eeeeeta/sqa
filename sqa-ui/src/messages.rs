use gtk::prelude::*;
use gtk::{Statusbar, Builder, ButtonsType, DialogFlags, MessageType, MessageDialog, Window};

pub enum Message {
    Statusbar(String),
    Error(String)
}
pub struct MessageController {
    ctxid: u32,
    messages: Vec<(u32, String)>,
    statusbar: Statusbar
}
impl MessageController {
    pub fn new(b: &Builder) -> Self {
        let ctxid = 0;
        let messages = vec![];
        let mut ctx = build!(MessageController using b
                             with ctxid, messages
                             get statusbar);
        ctx.ctxid = ctx.statusbar.get_context_id("default");
        ctx.statusbar.push(ctx.ctxid, "Welcome to SQA!");
        ctx
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
                MessageDialog::new(None::<&Window>,
                                   DialogFlags::empty(),
                                   MessageType::Error,
                                   ButtonsType::Close,
                                   &st).run();
            }
        }
    }
}
