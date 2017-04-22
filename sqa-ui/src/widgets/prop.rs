use gtk::prelude::*;
use gtk::{Window, Box, ButtonBox, Button, Label, Image, Builder, IsA, Widget, Orientation, IconSize};

pub struct PropertyWindow {
    pub window: Window,
    pub header_lbl: Label,
    pub subheader_lbl: Label,
    pub header_img: Image,
    pub props_box: Box,
    pub button_box: ButtonBox
}
impl PropertyWindow {
    pub fn new(b: &Builder) -> Self {
        build!(PropertyWindow using b
               get window, header_lbl, subheader_lbl, header_img,
               props_box, button_box)
    }
    pub fn update_header<T: AsRef<str>, U: AsRef<str>>(&mut self, img_stock: &str, header: T, subheader: U) {
        self.set_stock_img(img_stock);
        self.header_lbl.set_text(header.as_ref());
        self.subheader_lbl.set_text(subheader.as_ref());
    }
    pub fn set_stock_img(&mut self, name: &str) {
        self.header_img.set_from_stock(name, IconSize::Dialog.into());
    }
    pub fn append_property<T: IsA<Widget>>(&mut self, text: &str, prop: &T) -> Label {
        use gtk::Align;
        let label = Label::new(Some(text));
        label.set_halign(Align::Start);
        let bx = Box::new(Orientation::Horizontal, 0);
        bx.pack_start(&label, true, true, 5);
        bx.pack_end(prop, false, true, 5);
        self.props_box.pack_start(&bx, false, true, 5);
        label
    }
    pub fn append_button(&mut self, btn: &Button) {
        self.button_box.pack_end(btn, false, false, 5);
    }
}