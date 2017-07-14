use gtk::prelude::*;
use gtk::Entry;
use std::cell::Cell;
use std::time::Duration;
use std::ops::Deref;
use sync::{UISender, UIMessage};

pub trait DurationEntryMessage {
    type Message: Into<UIMessage>;
    type Identifier: Copy + 'static;

    fn on_payload(dur: Duration, id: Self::Identifier) -> Self::Message;
}
pub struct DurationEntry {
    dur: Cell<Duration>,
    ent: Entry,
}
macro_rules! try_or_none {
    ($e:expr) => {
        match $e {
            Ok(x) => x,
            Err(_) => return None
        }
    }
}
impl DurationEntry {
    pub fn new() -> Self {
        let dur = Cell::new(Default::default());
        let ent = Entry::new();
        Self { dur, ent }
    }
    pub fn bind<T: DurationEntryMessage>(&self, tx: &UISender, id: T::Identifier) {
        let dc = &self.dur;
        self.ent.connect_activate(clone!(tx, id, dc; |slf| {
            trace!("duration entry activated");
            if let Some(dur) = Self::parse(&slf.get_text().unwrap_or("".into())) {
                if dc.get() != dur {
                    dc.set(dur);
                    tx.send_internal(T::on_payload(dur, id));
                    trace!("new duration: {:?}", dur);
                }
            }
        }));
        self.ent.connect_focus_out_event(clone!(tx, id, dc; |slf, _| {
            if let Some(dur) = Self::parse(&slf.get_text().unwrap_or("".into())) {
                if dc.get() != dur {
                    dc.set(dur);
                    tx.send_internal(T::on_payload(dur, id));
                    trace!("new focusout duration: {:?}", dur);
                }
            }
            else {
                trace!("resetting duration to {:?} because input was bad", dc.get());
                slf.set_text(&Self::format(dc.get(), true));
            }
            Inhibit(false)
        }));
    }
    pub fn format(dur: Duration, show_millis: bool) -> String {
        let secs = dur.as_secs();
        let hrs = secs / 3600;
        let mins = (secs / 60).saturating_sub(60 * hrs);
        let just_secs = secs.saturating_sub(60 * mins).saturating_sub(3600 * hrs);
        let millis = dur.subsec_nanos() as u64 / 1_000_000;
        let hrs = if hrs > 0 {
            format!("{:02}:", hrs)
        }
        else {
            "".to_string()
        };
        trace!("format orig_secs {} h{} m{} s{} mi{}", secs, hrs, mins, just_secs, millis);
        if show_millis {
            format!("{}{:02}:{:02}.{:03}", hrs, mins, just_secs, millis)
        }
        else {
            format!("{}{:02}:{:02}", hrs, mins, just_secs)
        }
    }
    pub fn parse(st: &str) -> Option<Duration> {
        let time_components = st.rsplit(":").collect::<Vec<_>>();
        let (mut secs, mut nanos) = (0, 0);
        if let Some(secs_and_millis) = time_components.get(0) {
            let secs_and_millis = try_or_none!(secs_and_millis.parse::<f64>());
            if !secs_and_millis.is_finite() {
                return None;
            }
            let s = secs_and_millis.trunc();
            let ms = (secs_and_millis - s) * 1000.0;
            trace!("parse s&m {} s {} ms {}", secs_and_millis, s, ms);
            secs += s as u64;
            nanos += (ms as u32) * 1_000_000;
        }
        if let Some(mins) = time_components.get(1) {
            secs += try_or_none!(mins.parse::<u64>()) * 60;
        }
        if let Some(hrs) = time_components.get(2) {
            secs += try_or_none!(hrs.parse::<u64>()) * 3600;
        }
        Some(Duration::new(secs, nanos))
    }
    pub fn set(&mut self, dur: Duration) {
        self.dur.set(dur);
        if !self.ent.has_focus() {
            self.ent.set_text(&Self::format(dur, true));
        }
        else {
            trace!("not updating; widget has focus");
        }
    }
}

impl Deref for DurationEntry {
    type Target = Entry;

    fn deref(&self) -> &Entry {
        &self.ent
    }
}
