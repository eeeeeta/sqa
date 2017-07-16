use gtk::prelude::*;
use gtk::{Statusbar, Builder, Button, ListBoxRow, Orientation, Image, ListBox, Box, Label};
use sync::UISender;

pub enum Message {
    Statusbar(String),
    Error(String),
    RowsChanged,
    ClearAll
}
pub struct MessageController {
    ctxid: u32,
    messages: Vec<(u32, String)>,
    list: ListBox,
    clear_all: ListBoxRow,
    statusbar: Statusbar,
    tx: Option<UISender>
}
impl MessageController {
    pub fn new(b: &Builder) -> Self {
        let ctxid = 0;
        let messages = vec![];
        let tx = None;
        let mut ctx = build!(MessageController using b
                             with ctxid, messages, tx
                             get list, statusbar, clear_all);
        ctx.clear_all.hide();
        ctx.ctxid = ctx.statusbar.get_context_id("default");
        ctx.statusbar.push(ctx.ctxid, "Welcome to SQA!");
        ctx
    }
    pub fn bind(&mut self, tx: &UISender) {
        self.list.connect_row_activated(clone!(tx; |_, _| {
            tx.send_internal(Message::ClearAll);
        }));
        self.tx = Some(tx.clone());
    }
    pub fn update(&mut self) {
        if self.list.get_children().len() == 1 {
            self.clear_all.hide();
        }
        else {
            self.clear_all.show_all();
        }
    }
    pub fn on_message(&mut self, msg: Message) {
        use self::Message::*;
        match msg {
            Statusbar(st) => {
                info!("statusbar message: {}", st);
                let id = self.statusbar.push(self.ctxid, &st);
                self.messages.push((id, st));
            },
            Error(st) => {
                warn!("warning message: {}", st);
                let lbr = ListBoxRow::new();
                let bx = Box::new(Orientation::Horizontal, 5);
                let image = Image::new_from_icon_name("gtk-dialog-warning", 5);
                let btn = Button::new_from_icon_name("gtk-close", 4);
                bx.pack_start(&image, false, false, 0);
                let lbl = Label::new(None);
                lbl.set_line_wrap(true);
                lbl.set_markup(&st);
                bx.pack_end(&btn, false, false, 0);
                bx.pack_end(&lbl, true, true, 0);
                bx.set_margin_bottom(5);
                bx.set_margin_left(5);
                bx.set_margin_right(5);
                bx.set_margin_top(5);
                lbr.add(&bx);
                lbr.set_activatable(false);
                lbr.set_selectable(false);
                let tx = self.tx.as_ref().unwrap().clone();
                btn.connect_clicked(clone!(lbr; |_| {
                    lbr.destroy();
                    tx.send_internal(Message::RowsChanged);
                }));
                self.list.add(&lbr);
                lbr.show_all();
                self.update();
            },
            RowsChanged => {
                self.update();
            },
            ClearAll => {
                for child in self.list.get_children().into_iter().skip(1).rev() {
                    child.destroy();
                }
                self.update();
            },
        }
    }
}
