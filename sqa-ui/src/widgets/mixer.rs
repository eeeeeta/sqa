use gtk::prelude::*;
use gtk::{Orientation, Grid, RadioButton, ToggleButton, Align, Label, Scale, Entry, PositionType, Inhibit};
use glib::signal;
use sync::{UISender, UIMessage};
use std::marker::PhantomData;

#[derive(Clone)]
pub enum PatchedSliderMessage {
    VolChanged(f32),
    PatchChanged(usize)
}
pub type FadedSliderMessage = Option<f32>;

pub trait SliderMessage<T: SliderBoxType> {
    type Message: Into<UIMessage>;
    type Identifier: Copy + 'static;

    fn on_payload(ch: usize, data: T::Message, id: Self::Identifier) -> Self::Message;
}
pub struct SliderDetail {
    pub vol: f64,
    pub patch: usize
}
pub type FadedSliderDetail = Option<f32>;
pub struct Slider<S: SliderBoxType, T: SliderMessage<S>> {
    name: String,
    idx: usize,
    id: T::Identifier,
    vol: Entry,
    tb: Option<ToggleButton>,
    radios: Vec<(RadioButton, u64)>,
    scale: Scale,
    changed_handler: u64,
    clicked_handler: u64,
}
pub struct Patched;
pub struct Faded;

mod detail {
    use super::{SliderMessage, SliderBox, Slider};
    pub trait SliderBoxType: Sized {
        type Detail;
        type Message;
        fn append_slider_extra<T: SliderMessage<Self>>(&mut SliderBox<Self, T>, &mut Slider<Self, T>) {
        }
        fn update_slider<T: SliderMessage<Self>>(&mut SliderBox<Self, T>, usize, Self::Detail) {
        }
    }
}
use self::detail::SliderBoxType;
impl SliderBoxType for Patched {
    type Detail = SliderDetail;
    type Message = PatchedSliderMessage;
    fn append_slider_extra<T: SliderMessage<Self>>(slf: &mut SliderBox<Self, T>, slider: &mut Slider<Self, T>) {
        let ref tx = slf.tx;
        for n in 0..(slf.n_output+1) {
            if slider.name == "master" {
                let lbl = Label::new(None);
                let name = if n == slf.n_output {
                    "?".to_string()
                } else {
                    format!("{}←", n+1)
                };
                lbl.set_markup(&name);
                slf.grid.attach(&lbl, slf.grid_left, (4+n) as i32, 1, 1);
            }
            else {
                let rb = if let Some(&(ref r, _)) = slider.radios.get(0) {
                    RadioButton::new_from_widget(r)
                } else {
                    RadioButton::new(&[])
                };
                let mut handler_id = 0;
                if n == slf.n_output {
                    rb.set_sensitive(false);
                }
                else {
                    let idx = slider.idx;
                    let id = slider.id;
                    handler_id = rb.connect_toggled(clone!(tx; |rb| {
                        if rb.get_active() {
                            tx.send_internal(T::on_payload(idx, PatchedSliderMessage::PatchChanged(n+1), id));
                        }
                    }));
                }
                rb.set_halign(Align::Center);
                slf.grid.attach(&rb, slf.grid_left, (4+n) as i32, 1, 1);
                slider.radios.push((rb, handler_id));
            }
        }
        let ref mut scale = slider.scale;
        let ref mut vol = slider.vol;
        let idx = slider.idx;
        let id = slider.id;
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
        slider.changed_handler = scale.connect_value_changed(clone!(tx; |slf| {
            let mut val = slf.get_value();
            if val == -60.0 {
                val = ::std::f64::NEG_INFINITY;
            }
            tx.send_internal(T::on_payload(idx, PatchedSliderMessage::VolChanged(val as f32), id));
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

    }
    fn update_slider<T: SliderMessage<Self>>(slf: &mut SliderBox<Self, T>, i: usize, val: SliderDetail) {
        if let Some(slider) = slf.sliders.get_mut(i) {
            if i != 0 {
                trace!("mixer: patch for value {} is {}, {:?}", i, val.patch, slider.radios.get(val.patch));
                for &(ref rb, hid) in slider.radios.iter().take(slider.radios.len()-1) {
                    signal::signal_handler_block(rb, hid);
                }
                let rb;
                if val.patch == 0 {
                    rb = &slider.radios[0].0;
                }
                else {
                    if let Some(&(ref rb2, _)) = slider.radios.get(val.patch-1) {
                        rb = rb2;
                    }
                    else {
                        rb = &slider.radios[0].0;
                    }
                }
                rb.set_active(true);
                for &(ref rb, hid) in slider.radios.iter().take(slider.radios.len()-1) {
                    signal::signal_handler_unblock(rb, hid);
                }
            }
            if val.vol != slider.scale.get_value() {
                signal::signal_handler_block(&slider.scale, slider.changed_handler);
                slider.scale.set_value(val.vol);
                signal::signal_handler_unblock(&slider.scale, slider.changed_handler);
            }
        }
    }
}
impl SliderBoxType for Faded {
    type Detail = FadedSliderDetail;
    type Message = FadedSliderMessage;
    fn append_slider_extra<T: SliderMessage<Self>>(slf: &mut SliderBox<Self, T>, slider: &mut Slider<Self, T>) {
        let sctx = slider.vol.get_style_context().unwrap();
        sctx.remove_class("vol-entry");
        sctx.add_class("vol-entry-fade");
        let ref mut scale = slider.scale;
        let ref mut vol = slider.vol;
        let tb = ToggleButton::new_with_label("ON");
        slf.grid.attach(&tb, slf.grid_left, 4, 1, 1);
        let ref tx = slf.tx;
        let idx = slider.idx;
        let id = slider.id;
        slider.clicked_handler = tb.connect_clicked(clone!(tx; |slf| {
            trace!("on button clicked, currently on: {}", slf.get_active());
            if slf.get_active() {
                tx.send_internal(T::on_payload(idx, Some(0.0), id));
            }
            else {
                tx.send_internal(T::on_payload(idx, None, id));
            }
        }));
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
        slider.changed_handler = scale.connect_value_changed(clone!(tx; |slf| {
            let mut val = slf.get_value() as f32;
            if val == -60.0 {
                val = ::std::f32::NEG_INFINITY;
            }
            tx.send_internal(T::on_payload(idx, Some(val as f32), id));
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
        slider.tb = Some(tb);
    }
    fn update_slider<T: SliderMessage<Self>>(slf: &mut SliderBox<Self, T>, i: usize, val: FadedSliderDetail) {
        if let Some(slider) = slf.sliders.get_mut(i) {
            signal::signal_handler_block(slider.tb.as_ref().unwrap(), slider.clicked_handler);
            signal::signal_handler_block(&slider.scale, slider.changed_handler);
            trace!("mixer: updating slider, {:?}", val);
            if let Some(val) = val {
                let val = val as f64;
                if val != slider.scale.get_value() {
                    slider.scale.set_value(val);
                }
                slider.tb.as_ref().unwrap().set_active(true);
                let sctx = slider.vol.get_style_context().unwrap();
                sctx.add_class("vol-entry-fade-enabled");
            }
            else {
                slider.scale.set_value(-60.0);
                slider.vol.set_text("");
                slider.tb.as_ref().unwrap().set_active(false);
                let sctx = slider.vol.get_style_context().unwrap();
                sctx.remove_class("vol-entry-fade-enabled");
            }
            signal::signal_handler_unblock(&slider.scale, slider.changed_handler);
            signal::signal_handler_unblock(slider.tb.as_ref().unwrap(), slider.clicked_handler);
        }
    }
}

pub struct SliderBox<T: SliderBoxType, U: SliderMessage<T>> {
    pub grid: Grid,
    tx: UISender,
    grid_left: i32,
    sliders: Vec<Slider<T, U>>,
    n_output: usize,
    _ph: PhantomData<T>
}
impl<A, T> SliderBox<A, T> where A: SliderBoxType, T: SliderMessage<A> {
    fn append_slider(&mut self, name: &str, idx: usize, id: T::Identifier) {
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
        self.grid.attach(&scale, self.grid_left, 1, 1, 1);
        self.grid.attach(&vol, self.grid_left, 2, 1, 1);
        self.grid.attach(&lbl, self.grid_left, 3, 1, 1);

        let mut slider = Slider { vol, radios: Vec::new(), scale, changed_handler: 0, clicked_handler: 0, name: name.to_string(), idx, id, tb: None };

        A::append_slider_extra::<T>(self, &mut slider);

        self.sliders.push(slider);
        self.grid_left += 1;
    }
    pub fn new(n_input: usize, n_output: usize, tx: &UISender, id: T::Identifier) -> Self {
        let grid = Grid::new();
        grid.set_column_spacing(5);
        grid.set_row_spacing(5);
        let mut ret = SliderBox {
            grid_left: 0,
            sliders: Vec::with_capacity(n_input),
            tx: tx.clone(),
            _ph: PhantomData,
            grid, n_output
        };
        ret.append_slider("master", 0, id);
        for n in 0..n_input {
            ret.append_slider(&format!("{}", n+1), n+1, id);
        }
        ret
    }
    pub fn n_output(&self) -> usize {
        self.n_output
    }
    pub fn n_sliders(&self) -> usize {
        self.sliders.len()-1
    }
    pub fn update_values(&mut self, values: Vec<A::Detail>) {
        for (i, val) in values.into_iter().enumerate() {
            A::update_slider(self, i, val);
        }
    }
}
