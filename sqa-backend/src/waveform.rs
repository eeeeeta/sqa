//! APIs for getting waveforms out of audio files.

use futures::sync::oneshot::{channel, Receiver};
use std::collections::HashMap;
use std::time::Duration;
use state::{Context, CD};
use uuid::Uuid;
use errors::*;
use actions::audio::Controller as AudioController;
use std::thread;
use futures::*;
use codec::Reply;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WaveformRequest {
    pub file: String,
    pub samples_per_pixel: u32,
    pub range_start: Option<Duration>,
    pub range_end: Option<Duration>
}
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct SampleOverview {
    pub rms: f32,
    pub max: f32,
    pub min: f32
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WaveformReply {
    pub data: Vec<SampleOverview>,
    pub gmax: f32,
    pub gmin: f32
}
pub struct WaveformContext {
    pub(crate) active: HashMap<Uuid, Receiver<BackendResult<WaveformReply>>>,
    completed: HashMap<Uuid, WaveformReply>
}

impl WaveformContext {
    pub fn new() -> Self {
        Self {
            active: HashMap::new(),
            completed: HashMap::new()
        }
    }
    fn calculate_rms(samples: &[f32]) -> f32 {
        let sqr_sum = samples.iter().fold(0.0, |sqr_sum, sample| {
            sqr_sum + sample * sample
        });
        (sqr_sum / samples.len() as f32).sqrt()
    }
    pub fn execute_request(ctx: &mut Context, d: &mut CD, uu: Uuid, req: WaveformRequest) -> BackendResult<()> {
        debug!("executing waveform request {}", uu);
        if ctx.waveform.active.get(&uu).is_some() {
            debug!("Waveform request is already in progress!");
            return Ok(());
        }
        if let Some(res) = ctx.waveform.completed.get(&uu) {
            debug!("Waveform request already completed, rebroadcasting");
            d.broadcast(Reply::WaveformGenerated { uuid: uu, res: Ok(res.clone()) })?;
            return Ok(());
        }
        let path = AudioController::parse_url(&req.file)?;
        debug!("opening: {}", path.to_string_lossy());
        let mut file = AudioController::open_url(&path, ctx)?;
        let dur = file.duration();
        let start = req.range_start.unwrap_or(Duration::new(0, 0));
        let end = req.range_end.unwrap_or(dur.to_std().unwrap());
        if start >= end {
            bail!("Waveform request start point is greater than or equal to end point.");
        }
        if req.samples_per_pixel == 0 {
            bail!("Waveform request asks for no samples.");
        }
        debug!("spawning new thread");
        let (tx, mut rx) = channel();
        rx.poll().unwrap();
        ctx.waveform.active.insert(uu, rx);
        thread::spawn(move || {
            let mut run = || -> BackendResult<WaveformReply> {
                debug!("thread executing waveform request {}", uu);
                debug!("{} samples per overview", req.samples_per_pixel);
                debug!("end: {:?}", end);
                let mut ret = vec![];
                let chans = file.channels();
                if req.range_start.is_some() {
                    file.seek(::time::Duration::from_std(start).unwrap())?;
                }
                let mut rms_range = vec![];
                let mut min = 0.0;
                let mut gmin = 0.0;
                let mut max = 0.0;
                let mut gmax = 0.0;
                'outer: for frame in &mut file {
                    match frame {
                        Ok(mut frame) => {
                            for (ch, sample) in &mut frame {
                                let sample = sample.f32();
                                if ch == 0 {
                                    if rms_range.len() as u32 >= req.samples_per_pixel {
                                        ret.push(SampleOverview {
                                            min, max,
                                            rms: Self::calculate_rms(&rms_range)
                                        });
                                        if max > gmax { gmax = max };
                                        if min < gmin { gmin = min };
                                        rms_range = vec![];
                                        min = 0.0;
                                        max = 0.0;
                                    }
                                    rms_range.push(sample / chans as f32);
                                }
                                else {
                                    *rms_range.last_mut().unwrap() += sample / chans as f32;
                                }
                                if sample > max { max = sample };
                                if sample < min { min = sample };
                            }
                        },
                        Err(e) => {
                            match *e.kind() {
                                ::sqa_ffmpeg::errors::ErrorKind::InvalidData => {
                                    debug!("Invalid data in waveform reader, not doing anything");
                                },
                                _ => bail!("File read error: {:?}", e)
                            }
                        }
                    }
                }
                debug!("done, produced {} samples", ret.len());
                Ok(WaveformReply { data: ret, gmax, gmin })
            };
            let _ = tx.send(run());
        });
        Ok(())
    }
    pub fn on_wakeup(ctx: &mut Context, d: &mut CD) -> BackendResult<()> {
        let mut completed = vec![];
        for (uu, rx) in ctx.waveform.active.iter_mut() {
            match rx.poll() {
                Ok(Async::Ready(x)) => completed.push((*uu, x)),
                Err(_) => completed.push((*uu, Err("Waveform thread died".into()))),
                _ => {}
            }
        }
        for (uu, res) in completed {
            debug!("waveform request {} resolved", uu);
            ctx.waveform.active.remove(&uu);
            if let Ok(ref res) = res {
                ctx.waveform.completed.insert(uu, res.clone());
            }
            d.broadcast(Reply::WaveformGenerated { uuid: uu, res: res.map_err(|x| x.to_string()) })?;
        }
        Ok(())
    }
}
