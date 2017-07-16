//! Plays back an audio file.

use sqa_engine::{PlainSender, BufferSender};
use sqa_engine::param::Parameter;
use sqa_engine::sync::AudioThreadMessage;
use sqa_ffmpeg::{Frame, MediaFile, MediaResult};
use sqa_ffmpeg::errors::ErrorKind;
use super::{ParameterError, ControllerParams, DurationInfo, PlaybackState, ActionController, EditableAction};
use state::{ServerMessage, Context, IntSender};
use std::thread;
use std::ops::Deref;
use errors::*;
use std::sync::mpsc::{Sender, Receiver, self};
use uuid::Uuid;
use url::percent_encoding;
use url::Url;
use std::time::Duration;
use std::path::{Path, PathBuf};

/// Converts a linear amplitude to decibels.
pub fn lin_db(lin: f32) -> f32 {
    lin.log10() * 20.0
}
/// Converts a decibel value to a linear amplitude.
pub fn db_lin(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}
pub enum SpoolerMessage {
    Wakeup,
    Quit
}
pub struct SpoolerContext {
    bsends: Vec<BufferSender>,
    file: MediaFile,
    uuid: Uuid,
    sender: IntSender,
    rx: Receiver<SpoolerMessage>,
}
impl SpoolerContext {
    pub fn send_frame(bsends: &mut Vec<BufferSender>, frame: &mut Frame) -> bool {
        let space = bsends[0].buf.capacity() - bsends[0].buf.size();
        for (ch, sample) in frame.take(space * bsends.len()) {
            bsends[ch].buf.try_push(sample.f32());
        }
        !frame.drained()
    }
    pub fn spool(&mut self) {
        let mut current_msg = None;
        let mut current_frame: Option<Frame> = None;
        'outer: loop {
            if let Some(m) = current_msg.take() {
                use self::SpoolerMessage::*;
                match m {
                    Wakeup => {},
                    Quit => return
                }
            }
            if self.bsends[0].buf.size() == self.bsends[0].buf.capacity() {
                current_msg = Some(self.rx.recv().unwrap());
                continue 'outer;
            }
            if let Some(mut frame) = current_frame.take() {
                if Self::send_frame(&mut self.bsends, &mut frame) {
                    current_frame = Some(frame);
                    current_msg = Some(self.rx.recv().unwrap());
                    continue 'outer;
                }
            }
            for frame in &mut self.file {
                match frame {
                    Ok(mut frame) => {
                        if Self::send_frame(&mut self.bsends, &mut frame) {
                            current_frame = Some(frame);
                            current_msg = Some(self.rx.recv().unwrap());
                            continue 'outer;
                        }
                    },
                    Err(e) => {
                        match *e.kind() {
                            ErrorKind::InvalidData => debug!("Invalid data in spooler, not doing anything"),
                            _ => {
                                error!("spooler error! {:?}", e);
                                let msg = format!("Error in spooler: {}", e);
                                self.sender.send(
                                    ServerMessage::ActionStateChange(self.uuid,
                                                                     PlaybackState::Errored(msg)));
                                return;
                            }
                        }
                    }
                }
            }
            // If we got here, we've sent all the frames!
            debug!("All frames sent, goodnight!");
            return;
        }
    }
}
pub struct RunningData {
    control: Sender<SpoolerMessage>,
    durinfo: DurationInfo,
    pub senders: Vec<PlainSender>
}
#[derive(Default)]
pub struct Controller {
    pub params: AudioParams,
    pub rd: Option<RunningData>,
    file: Option<MediaResult<MediaFile>>,
    url: Option<BackendResult<PathBuf>>
}
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AudioChannel {
    pub patch: Option<Uuid>,
    pub vol: f32
}
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AudioParams {
    pub url: Option<String>,
    pub chans: Vec<AudioChannel>,
    pub master_vol: f32
}
impl Controller {
    pub fn new() -> Self {
        Default::default()
    }
    pub fn parse_url(st: &str) -> BackendResult<PathBuf> {
        let url = Url::parse(st)?;
        if url.scheme() != "file" {
            bail!(format!("The URL scheme {} is not yet supported; only file:// URLs currently work.", url.scheme()));
        }
        let path = url.path();
        let st = percent_encoding::percent_decode(path.as_bytes()).decode_utf8_lossy();
        let path = Path::new(st.deref()).to_path_buf();
        if path.file_name().is_none() {
            bail!(format!("The URL provided contains no file name."));
        }
        Ok(path)
    }
    fn open_file(&mut self, ctx: &mut Context) -> Option<MediaResult<MediaFile>> {
        if let Some(ref uri2) = self.url {
            let uri;
            if let Ok(ref u) = *uri2 {
                uri = u;
            }
            else { return None; }
            let uri = uri.to_string_lossy();
            let mf = MediaFile::new(&mut ctx.media, &uri);
            match mf {
                Err(e) => Some(Err(e)),
                Ok(mf) => {
                    Some(Ok(mf))
                }
            }
        }
        else {
            None
        }
    }
}
impl EditableAction for Controller {
    type Parameters = AudioParams;

    fn get_params(&self) -> &AudioParams {
        &self.params
    }
    fn set_params(&mut self, mut p: AudioParams, ctx: ControllerParams) {
        if self.params.url != p.url {
            self.url = match p.url {
                Some(ref u) => Some(Self::parse_url(u)),
                None => None
            };
            self.file = self.open_file(ctx.ctx);
            if let Some(Ok(ref mf)) = self.file {
                if p.chans.len() != mf.channels() {
                    if p.chans.len() == 0 {
                        p.chans = (0..mf.channels())
                            .map(|idx| ctx.ctx.mixer.obtain_def(idx))
                            .map(|patch| AudioChannel { patch, .. Default::default() })
                            .collect::<Vec<_>>();
                    }
                    else if p.chans.len() < mf.channels() {
                        let len = p.chans.len();
                        p.chans.extend(::std::iter::repeat(Default::default())
                                       .take(mf.channels() - len));
                    }
                }
            }
        }
        if let Some(ref mut rd) = self.rd {
            for (i, ch) in p.chans.iter().enumerate() {
                if let Some(s) = rd.senders.get_mut(i) {
                    s.set_volume(Box::new(Parameter::Raw(db_lin(ch.vol))));
                }
            }
            rd.senders[0].set_master_volume(Box::new(Parameter::Raw(db_lin(p.master_vol))));
        }
        self.params = p;
    }
}
impl ActionController for Controller {
    fn desc(&self, _: &Context) -> String {
        if let Some(Ok(ref url)) = self.url {
            format!("{}", url.file_name().unwrap().to_string_lossy())
        }
        else {
            format!("[invalid audio cue]")
        }
    }
    fn verify_params(&self, ctx: &Context) -> Vec<ParameterError> {
        let mut ret = vec![];
        if self.params.url.is_some() {
            if let Some(Err(ref e)) = self.url {
                return vec![ParameterError {
                    name: "url".into(),
                    err: format!("Invalid URL: {}", e)
                }];
            }
            let mf = match self.file.as_ref() {
                Some(f) => f,
                None => {
                    return vec![ParameterError {
                        name: "url".into(),
                        err: format!("Internal error: no file opened!")
                    }]
                }
            };
            match *mf {
                Err(ref e) => {
                    ret.push(ParameterError {
                        name: "url".into(),
                        err: format!("Error opening URL: {}", e)
                    })
                },
                Ok(ref mf) => {
                    if mf.channels() == 0 {
                        ret.push(ParameterError {
                            name: "url".into(),
                            err: "What sort of file has exactly ZERO channels in it?!".into()
                        });
                    }
                    if self.params.chans.len() > mf.channels() {
                        ret.push(ParameterError {
                            name: "chans".into(),
                            err: "The file has fewer channels than expected (FIXME: better error message here)".into()
                        });
                    }
                }
            }
            for (i, ch) in self.params.chans.iter().enumerate() {
                if let Some(ref ch) = ch.patch {
                    if ctx.mixer.obtain_channel(&ch).is_none() {
                        ret.push(ParameterError {
                            name: "chans".into(),
                            err: format!("Channel {} does not exist.", i)
                        });
                    }
                }
            }
        }
        else {
            ret.push(ParameterError {
                name: "url".into(),
                err: "This field is required.".into()
            })
        }
        ret
    }
    fn load(&mut self, params: ControllerParams) -> BackendResult<bool> {
        let mf = self.file.take().ok_or("File mysteriously disappeared")??;
        self.file = self.open_file(params.ctx);
        let mut senders = params.ctx.mixer.new_senders(mf.channels(), mf.sample_rate() as u64);
        for (i, s) in senders.iter_mut().enumerate() {
            if let Some(ch) = self.params.chans.get(i) {
                if let Some(ref ch) = ch.patch {
                    let ch = params.ctx.mixer.obtain_channel(&ch)
                        .ok_or("One channel mysteriously disappeared")?;
                    s.set_output_patch(ch);
                }
            }
        }
        let dur = mf.duration().to_std().unwrap();
        let plains: Vec<PlainSender> = senders.iter()
            .map(|s| s.make_plain())
            .collect();
        let (tx, rx) = mpsc::channel();
        let mut sctx = SpoolerContext {
            bsends: senders,
            file: mf,
            uuid: params.uuid,
            sender: params.internal_tx.clone(),
            rx: rx
        };
        thread::spawn(move || {
            sctx.spool();
        });
        self.rd = Some(RunningData {
            senders: plains,
            control: tx,
            durinfo: DurationInfo {
                start_time: 0,
                duration: Duration::new(0, 0),
                est_duration: Some(dur)
            }
        });
        Ok(true)
    }
    fn execute(&mut self, time: u64, _: ControllerParams) -> BackendResult<bool> {
        if let Some(ref mut rd) = self.rd {
            rd.durinfo.start_time = time;
            for (i, sender) in rd.senders.iter_mut().enumerate() {
                if let Some(ch) = self.params.chans.get(i) {
                    sender.set_volume(Box::new(Parameter::Raw(db_lin(ch.vol))));
                }
                sender.set_start_time(time);
                sender.set_active(true);
            }
        }
        Ok(false)
    }
    fn pause(&mut self, _: ControllerParams) {
        if let Some(ref mut rd) = self.rd {
            for sender in rd.senders.iter_mut() {
                sender.set_active(false);
            }
        }
    }
    fn reset(&mut self, _: ControllerParams) {
        if let Some(rd) = self.rd.take() {
            let _ = rd.control.send(SpoolerMessage::Quit);
        }
    }
    fn duration_info(&self) -> Option<DurationInfo> {
        if let Some(ref rd) = self.rd {
            let mut ret = rd.durinfo;
            ret.duration = rd.senders[0].position().to_std().unwrap_or_else(|_| {
                error!("Hm, we actually failed to convert a Duration to std...");
                Duration::new(0, 0)
            });
            Some(ret)
        }
        else {
            None
        }
    }
    fn accept_audio_message(&mut self, msg: &AudioThreadMessage, ctx: ControllerParams) -> bool {
        use self::AudioThreadMessage::*;
        match *msg {
            PlayerBufHalf(uu) | PlayerBufEmpty(uu) => {
                if let Some(ref mut rd) = self.rd {
                    for sender in rd.senders.iter() {
                        if sender.uuid() == uu {
                            if let Err(e) = rd.control.send(SpoolerMessage::Wakeup) {
                                let msg = format!("Failed to wakeup spooler thread: {:?}", e);
                                ctx.internal_tx.send(ServerMessage::ActionStateChange(ctx.uuid, PlaybackState::Errored(msg)));
                            }
                            return true;
                        }
                    }
                }
                false
            },
            PlayerRemoved(ref pl) => {
                if let Some(ref mut rd) = self.rd {
                    if rd.senders[0].uuid() == pl.uuid {
                        debug!("We're done here.");
                        ctx.internal_tx.send(ServerMessage::ActionStateChange(ctx.uuid, PlaybackState::Inactive));
                        return true;
                    }
                }
                false
            },
            _ => false
        }
    }
}
