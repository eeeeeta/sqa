use gtk::prelude::*;
use gtk::{Box, Orientation, Separator, Align, Label, Scale, Entry, PositionType, Inhibit};
use std::rc::Rc;
use std::cell::Cell;
use glib::signal;
use sync::{UISender, UIMessage};

pub trait SliderMessage {
    type Message: Into<UIMessage>;
    type Identifier: Copy + 'static;

    fn vol_changed(ch: usize, val: f64, id: Self::Identifier) -> Self::Message;
    fn patch_changed(ch: usize, patch: usize, id: Self::Identifier) -> Self::Message;
}
pub struct SliderDetail {
    pub vol: f64,
    pub patch: usize
}
struct Slider {
    bx: Box,
    lbl: Label,
    vol: Entry,
    patch: Entry,
    scale: Scale,
    changed_handler: u64,
    patch_data: Rc<Cell<usize>>
}
pub struct SliderBox {
    pub cont: Box,
    sliders: Vec<Slider>,
    tx: UISender
}
impl SliderBox {
    pub fn new<T: SliderMessage>(n_input: usize, tx: UISender, id: T::Identifier) -> Self {
        let cont = Box::new(Orientation::Horizontal, 5);
        if n_input == 0 {
            let lbl = Label::new(Some("(No channels are currently defined.)"));
            lbl.set_halign(Align::Center);
            cont.pack_start(&lbl, true, true, 0);
            let sliders = Vec::new();
            return SliderBox { cont, sliders, tx };
        }
        let mut sliders = Vec::with_capacity(n_input);
        for n in 0..n_input {
            let bx = Box::new(Orientation::Vertical, 5);
            let lbl = Label::new(None);
            lbl.set_markup(&format!("<i>{}</i>", (n+1)));
            let vol = Entry::new();
            let patch = Entry::new();
            let sep = Separator::new(Orientation::Horizontal);
            let scale = Scale::new_with_range(Orientation::Vertical, -60.0, 12.0, 1.0);
            vol.set_has_frame(false);
            patch.set_has_frame(false);
            lbl.set_halign(Align::Center);
            vol.set_halign(Align::Center);
            vol.set_alignment(0.5);
            vol.set_width_chars(5);
            patch.set_halign(Align::Center);
            patch.set_alignment(0.5);
            patch.set_width_chars(5);
            scale.set_halign(Align::Center);
            scale.set_draw_value(false);
            scale.set_inverted(true);
            scale.set_size_request(-1, 160);
            scale.add_mark(0.0, PositionType::Right, None);
            bx.pack_start(&lbl, false, false, 0);
            bx.pack_start(&sep, false, false, 0);
            bx.pack_start(&vol, false, false, 0);
            bx.pack_start(&scale, true, true, 0);
            scale.set_vexpand(true);
            bx.pack_start(&patch, false, false, 0);

            let patch_data = Rc::new(Cell::new(0));
            let changed_handler = scale.connect_value_changed(clone!(vol, tx; |slf| {
                println!("scale value changed for ch {} to {}", n, slf.get_value());
                let val = slf.get_value();
                vol.set_text(&format!("{:.2}", val));
                tx.send_internal(T::vol_changed(n, val, id));
            }));
            vol.connect_activate(clone!(scale, tx; |slf| {
                if let Ok(val) = slf.get_text().unwrap_or("".into()).parse() {
                    scale.set_value(val);
                    tx.send_internal(T::vol_changed(n, val, id));
                }
            }));
            vol.connect_focus_out_event(clone!(scale, tx; |slf, _e| {
                if let Ok(val) = slf.get_text().unwrap_or("".into()).parse() {
                    scale.set_value(val);
                    tx.send_internal(T::vol_changed(n, val, id));
                }
                else {
                    scale.set_value(scale.get_value());
                }
                Inhibit(false)
            }));
            patch.connect_activate(clone!(patch_data, tx; |slf| {
                if let Ok(val) = slf.get_text().unwrap_or("".into()).parse() {
                    patch_data.set(val);
                    tx.send_internal(T::patch_changed(n, val, id));
                }
            }));
            patch.connect_focus_out_event(clone!(patch_data, tx; |slf, _e| {
                if let Ok(val) = slf.get_text().unwrap_or("".into()).parse() {
                    patch_data.set(val);
                    tx.send_internal(T::patch_changed(n, val, id));
                }
                else {
                    slf.set_text(&format!("{}", patch_data.get()));
                }
                Inhibit(false)
            }));
            patch.set_text("0");
            scale.set_value(0.0);
            cont.pack_start(&bx, false, false, 0);
            sliders.push(Slider { bx, lbl, vol, patch, scale, patch_data, changed_handler });
        }
        SliderBox { cont, sliders, tx }
    }
    pub fn n_sliders(&self) -> usize {
        self.sliders.len()
    }
    pub fn update_values(&mut self, values: Vec<SliderDetail>) {
        for (i, val) in values.into_iter().enumerate() {
            if let Some(slider) = self.sliders.get_mut(i) {
                if val.patch != slider.patch_data.get() {
                    slider.patch_data.set(val.patch);
                    slider.patch.set_text(&format!("{}", val.patch));
                }
                if val.vol != slider.scale.get_value() {
                    signal::signal_handler_block(&slider.scale, slider.changed_handler);
                    slider.scale.set_value(val.vol);
                    signal::signal_handler_unblock(&slider.scale, slider.changed_handler);
                }
            }
        }
    }
}
