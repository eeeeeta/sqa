use gtk::prelude::*;
use gtk::{FileChooserAction, FileChooserButton, DrawingArea, Widget};
use cairo::enums::{FontSlant, FontWeight};
use super::{ActionUI, OpaqueAction, UITemplate, ActionMessageInner};
use sync::UISender;
use uuid::Uuid;
use widgets::{SliderBox, Patched, PatchedSliderMessage, SliderMessage, SliderDetail};
use sqa_backend::codec::Command;
use sqa_backend::mixer::MixerConf;
use sqa_backend::actions::{ActionParameters, PlaybackState};
use sqa_backend::actions::audio::AudioParams;
use sqa_backend::waveform::{SampleOverview, WaveformReply, WaveformRequest};
use std::rc::Rc;
use std::sync::RwLock;

#[derive(Clone)]
pub enum AudioMessage {
    Slider(usize, PatchedSliderMessage),
    FileChanged,
    GenerateWaveform
}
impl SliderMessage<Patched> for AudioMessage {
    type Message = (Uuid, ActionMessageInner);
    type Identifier = Uuid;

    fn on_payload(ch: usize, data: PatchedSliderMessage, id: Uuid) -> Self::Message {
        use self::ActionMessageInner::*;
        use self::AudioMessage::*;
        (id, Audio(Slider(ch, data)))
    }
}
pub struct AudioUI {
    file: FileChooserButton,
    params: AudioParams,
    temp: UITemplate,
    cnf: MixerConf,
    sb: SliderBox<Patched, AudioMessage>,
    da: DrawingArea,
    waveform: Rc<RwLock<Option<WaveformReply>>>,
    cur_req: Option<Uuid>
}

impl AudioUI {
    pub fn new(uu: Uuid, tx: UISender) -> Self {
        let file = FileChooserButton::new("Audio file", FileChooserAction::Open);
        let mut temp = UITemplate::new(uu, tx.clone());
        let params = Default::default();
        let cnf = Default::default();
        let da = DrawingArea::new();
        let sb = SliderBox::new(0, 0, &tx, uu);
        let waveform = Rc::new(RwLock::new(None));
        let cur_req = None;
        temp.get_tab("Basics").append_property("File target", &file);
        let patch = temp.add_tab("Levels &amp; Patch");
        patch.container.pack_start(&sb.grid, false, true, 5);
        let wave = temp.add_tab("Waveform");
        wave.container.pack_start(&da, false, true, 5);
        da.set_vexpand(true);
        da.set_hexpand(true);
        let mut ctx = AudioUI { file, temp, params, cnf, sb, da, waveform, cur_req };
        ctx.bind();
        ctx
    }
    fn bind(&mut self) {
        self.temp.bind();
        let uu = self.temp.uu;
        let ref tx = self.temp.tx;
        self.file.connect_file_set(clone!(tx; |_| {
            tx.send_internal((uu, ActionMessageInner::Audio(AudioMessage::FileChanged)));
        }));
        let waveform = self.waveform.clone();
        self.da.connect_draw(clone!(tx; |slf, cr| {
            debug!("drawing waveform");
            let allocation = slf.get_allocation();
            let mid = allocation.height as f64 / 2.0;
            trace!("allocation: {:?}", allocation);
            cr.set_source_rgba(0.0, 0.19, 0.22, 1.0);
            cr.paint();
            cr.set_line_width(1.0);
            let wvf = waveform.read().unwrap();
            if let Some(ref wvf) = *wvf {
                cr.set_source_rgba(0.67, 0.56, 0.95, 1.0);
                let bar_width = allocation.width as f64 / wvf.data.len() as f64;
                let mut x = 0.0;
                for &SampleOverview { rms, max, min } in wvf.data.iter() {
                    cr.set_source_rgba(0.67, 0.56, 0.95, 1.0);
                    let y1 = mid - (max as f64 * mid);
                    let y2 = mid - (min as f64 * mid);
                    cr.move_to(x, y1);
                    cr.line_to(x + bar_width, y1);
                    cr.line_to(x + bar_width, y2);
                    cr.line_to(x, y2);
                    cr.close_path();
                    cr.fill();
                    cr.set_source_rgba(0.95, 0.26, 0.21, 1.0);
                    let y1 = mid - (rms as f64 * mid);
                    let y2 = mid - (-(rms as f64) * mid);
                    cr.move_to(x, y1);
                    cr.line_to(x + bar_width, y1);
                    cr.line_to(x + bar_width, y2);
                    cr.line_to(x, y2);
                    cr.close_path();
                    cr.fill();
                    x += bar_width;
                }
                cr.set_source_rgba(0.67, 0.56, 0.95, 1.0);
                cr.move_to(0.0, mid);
                cr.line_to(allocation.width as _, mid);
                cr.stroke();
            }
            else {
                cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
                let text = "No waveform yet";
                cr.set_font_size(52.0);
                let extents = cr.text_extents(text);
                let xc = (allocation.width / 2) as f64 - (extents.width / 2.0 + extents.x_bearing);
                let yc = (allocation.height / 2) as f64 - (extents.height / 2.0 + extents.y_bearing);
                trace!("text xc, yc = {}, {}", xc, yc);
                cr.select_font_face("Sans", FontSlant::Normal, FontWeight::Normal);
                cr.move_to(xc, yc);
                cr.show_text(text);
                tx.send_internal((uu, ActionMessageInner::Audio(AudioMessage::GenerateWaveform)));
            }
            Inhibit(false)
        }));
    }
    fn on_new_parameters(&mut self, p: &AudioParams) {
        trace!("audio: new parameters {:?}", p);
        if let Some(ref uri) = p.url {
            self.file.set_uri(uri);
        }
        else {
            self.file.unselect_all();
        }
        self.params = p.clone();
        if p.chans.len() != self.sb.n_sliders() || self.cnf.defs.len() != self.sb.n_output() {
            trace!("audio: recreating sliders!");
            self.sb.grid.destroy();
            self.sb = SliderBox::new(p.chans.len(), self.cnf.defs.len(), &self.temp.tx, self.temp.uu);
            let tab = self.temp.get_tab("Levels &amp; Patch");
            tab.container.pack_start(&self.sb.grid, false, true, 5);
            tab.container.show_all();
        }
        let mut details = p.chans.iter()
            .map(|ch| {
                let idx = if let Some(patch) = ch.patch {
                    self.cnf.defs.iter().position(|&p| p == patch).map(|x| x+1).unwrap_or(0)
                } else { 0 };
                SliderDetail { vol: ch.vol as f64, patch: idx }
            })
            .collect::<Vec<_>>();
        details.insert(0, SliderDetail { vol: p.master_vol as f64, patch: 0 });
        self.sb.update_values(details);
    }
    fn apply_changes<T: Into<String>>(&mut self, desc: T) {
        trace!("audio: sending update {:?}", self.params);
        self.temp.tx.send(Command::UpdateActionParams {
            uuid: self.temp.uu,
            params: ActionParameters::Audio(self.params.clone()),
            desc: Some(desc.into())
        });
    }
}
impl ActionUI for AudioUI {
    fn on_update(&mut self, p: &OpaqueAction) {
        self.temp.on_update(p);
        if let ActionParameters::Audio(ref pars) = p.params {
            self.on_new_parameters(pars);

            if let PlaybackState::Unverified(ref errs) = p.state {
                for err in errs {
                    if err.name == "url" {
                    }
                }
            }
        }
    }
    fn close_window(&mut self) {
        self.temp.pwin.hide();
    }
    fn on_message(&mut self, m: ActionMessageInner) {
        if let ActionMessageInner::Audio(m) = m {
            use self::AudioMessage::*;
            match m {
                FileChanged => {
                    self.params.url = self.file.get_uri();
                    self.apply_changes("change audio file target");
                },
                GenerateWaveform => {
                    if let Some(ref uri) = self.params.url {
                        if self.cur_req.is_none() {
                            let uuid = Uuid::new_v4();
                            debug!("sending new waveform request, uuid {}", uuid);
                            self.cur_req = Some(uuid);
                            self.temp.tx.send(Command::GenerateWaveform {
                                uuid,
                                req: WaveformRequest {
                                    file: uri.clone(),
                                    samples_per_pixel: 44_100,
                                    range_start: None,
                                    range_end: None
                                }
                            });
                        }
                    }
                },
                Slider(ch, PatchedSliderMessage::VolChanged(val)) => {
                    trace!("audio: slider, ch {} val {}", ch, val);
                    if ch == 0 {
                        self.params.master_vol = val;
                    }
                    else {
                        if self.params.chans.get(ch-1).is_some() {
                            self.params.chans[ch-1].vol = val;
                        }
                    }
                    self.apply_changes("change slider value");
                },
                Slider(ch, PatchedSliderMessage::PatchChanged(patch)) => {
                    if patch == 0 || ch == 0 { return }
                    let ch = ch - 1;
                    trace!("audio: setting patch for {}: {}, defs: {:?}", ch, patch, self.cnf.defs);
                    let patch = match self.cnf.defs.get(patch-1) {
                        Some(&u) => u,
                        None => return
                    };
                    if self.params.chans.get(ch).is_some() {
                        self.params.chans[ch].patch = Some(patch);
                        self.apply_changes("change patch");
                    }
                }
            }
        }

    }
    fn edit_separately(&mut self) {
        self.temp.edit_separately();
    }
    fn on_mixer(&mut self, cnf: &MixerConf) {
        self.cnf = cnf.clone();
        let p = self.params.clone();
        self.on_new_parameters(&p);
    }
    fn get_container(&mut self) -> Option<Widget> {
        self.temp.get_container()
    }
    fn change_cur_page(&mut self, cp: Option<u32>) {
        self.temp.change_cur_page(cp)
    }
    fn on_waveform_reply(&mut self, uu: Uuid, rpl: &WaveformReply) {
        if let Some(wait) = self.cur_req {
            if wait == uu {
                debug!("Got waveform reply! Updating...");
                *self.waveform.write().unwrap() = Some(rpl.clone());
                self.da.queue_draw();
            }
        }
    }
}
