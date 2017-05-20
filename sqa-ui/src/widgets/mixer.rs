use gtk::prelude::*;
use gtk::{Orientation, Grid, RadioButton, Align, Label, Scale, Entry, PositionType, Inhibit};
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
    lbl: Label,
    vol: Entry,
    radios: Vec<RadioButton>,
    scale: Scale,
    changed_handler: u64
}
pub struct SliderBox {
    pub grid: Grid,
    sliders: Vec<Slider>,
    n_output: usize
}
impl SliderBox {
    fn append_slider<T: SliderMessage>(name: &str, idx: usize, n_output: usize, grid: &Grid, grid_left: i32, id: T::Identifier, tx: &UISender) -> Slider {
        let lbl = Label::new(None);
        lbl.set_markup(name);
        let vol = Entry::new();
        let scale = Scale::new_with_range(Orientation::Vertical, -60.0, 12.0, 1.0);
        vol.set_has_frame(false);
        lbl.set_halign(Align::Center);
        vol.set_halign(Align::Center);
        vol.set_width_chars(5);
        vol.set_alignment(0.5);
        scale.set_halign(Align::Center);
        scale.set_draw_value(false);
        scale.set_inverted(true);
        scale.set_size_request(-1, 120);
        scale.add_mark(0.0, PositionType::Right, None);
        scale.set_vexpand(true);
        vol.get_style_context().unwrap().add_class("vol-entry");
        grid.attach(&scale, grid_left, 1, 1, 1);
        grid.attach(&vol, grid_left, 2, 1, 1);
        grid.attach(&lbl, grid_left, 3, 1, 1);

        let mut radios = vec![];
        for n in 0..(n_output+1) {
            if name == "master" {
                let lbl = Label::new(None);
                let name = if n == n_output {
                    "?".to_string()
                } else {
                    format!("{}←", n+1)
                };
                lbl.set_markup(&name);
                grid.attach(&lbl, grid_left, (4+n) as i32, 1, 1);
            }
            else {
                let rb = if let Some(r) = radios.get(0) {
                    RadioButton::new_from_widget(r)
                } else {
                    RadioButton::new(&[])
                };
                if n == n_output {
                    rb.set_sensitive(false);
                }
                else {
                    rb.connect_toggled(clone!(tx; |slf| {
                        if slf.get_active() {
                            tx.send_internal(T::patch_changed(idx, n+1, id));
                        }
                    }));
                }
                rb.set_halign(Align::Center);
                grid.attach(&rb, grid_left, (4+n) as i32, 1, 1);
                radios.push(rb);
            }
        }
        scale.connect_value_changed(clone!(vol; |slf| {
            let val = slf.get_value();
            if val == -60.0 {
                vol.set_text("-inf");
            }
            else {
                let prec = if val.trunc() == val { 0 } else { 2 };
                let sign = if val > 0.0 { "+" } else { "" };
                vol.set_text(&format!("{}{:.*}", sign, prec, val));
            }
        }));
        let changed_handler = scale.connect_value_changed(clone!(tx; |slf| {
            let mut val = slf.get_value();
            if val == -60.0 {
                val = ::std::f64::NEG_INFINITY;
            }
            tx.send_internal(T::vol_changed(idx, val, id));
        }));
        vol.connect_activate(clone!(scale; |slf| {
            if let Ok(val) = slf.get_text().unwrap_or("".into()).parse() {
                scale.set_value(val);
            }
        }));
        vol.connect_focus_out_event(clone!(scale; |slf, _e| {
            if let Ok(val) = slf.get_text().unwrap_or("".into()).parse() {
                scale.set_value(val);
            }
            else {
                scale.set_value(scale.get_value());
            }
            Inhibit(false)
        }));
        Slider { lbl, vol, radios, scale, changed_handler }
    }
    pub fn new<T: SliderMessage>(n_input: usize, n_output: usize, tx: &UISender, id: T::Identifier) -> Self {
        let grid = Grid::new();
        grid.set_column_spacing(5);
        grid.set_row_spacing(5);
        let mut sliders = Vec::with_capacity(n_input);
        sliders.push(Self::append_slider::<T>("master", 0, n_output, &grid, 1, id, &tx));
        for n in 0..n_input {
            sliders.push(Self::append_slider::<T>(&format!("{}", n+1), n+1, n_output, &grid, (n+2) as i32, id, &tx));
        }
        SliderBox { grid, sliders, n_output }
    }
    pub fn n_output(&self) -> usize {
        self.n_output
    }
    pub fn n_sliders(&self) -> usize {
        self.sliders.len()-1
    }
    pub fn update_values(&mut self, values: Vec<SliderDetail>) {
        for (i, val) in values.into_iter().enumerate() {
            if let Some(slider) = self.sliders.get_mut(i) {
                if i != 0 {
                    println!("patch for value {} is {}, {:?}", i, val.patch, slider.radios.get(val.patch));
                    let rb;
                    if val.patch == 0 {
                        rb = &slider.radios[0];
                    }
                    else {
                        if let Some(rb2) = slider.radios.get(val.patch-1) {
                            rb = rb2;
                        }
                        else {
                            rb = &slider.radios[0];
                        }
                    }
                    rb.set_active(true);
                }
                if val.vol != slider.scale.get_value() {
                    println!("blocking changed handler");
                    signal::signal_handler_block(&slider.scale, slider.changed_handler);
                    slider.scale.set_value(val.vol);
                    println!("unblocking changed handler");
                    signal::signal_handler_unblock(&slider.scale, slider.changed_handler);
                }
            }
        }
    }
}
