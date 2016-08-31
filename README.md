SQA: stuttery QLab alternative
==============================
*first public beta*

![Screenshot of SQA beta 1](https://i.imgur.com/nDHZjsY.png)

*SQA beta 1, showing off some of its features: the command line editing system, cues, the identifier naming system...*

## wat

This project aims to be an audio player & cue system for live shows and staged productions,
à la Figure53's [QLab](http://figure53.com/qlab/). It follows a command-based model, where every action in
SQA is a command that can either be executed immediately or stored in a *cue* (a list of commands) to be
run as part of a live show at the press of a button.

All its code is written in the [Rust programming language](https://www.rust-lang.org/), a new language
that prevents memory unsafety and improves programming ergonomics. 

## Cool! Does it work?

Yes, although there are most definitely rough edges in its user experience (and almost everywhere else :P).
Stuff it supports right now, in this beta:

- Loading MP3 files (and MP3 files only!), playing them back, stopping, pausing, restarting (no seek yet!)
- Adjusting volume, including nice fading
- Cues (which are just lists of commands), including the ability to "fall through" commands instead of
  waiting for them to finish execution

(Yeah...that's not that much :P)

Stuff that is theoretically possible with minor tweaks that I haven't had time to implement yet:

- Dynamic selection of output (right now, it'll just use your system default audio output)
- Advanced patch system, and wiring (it's in the backend, there just isn't a nice UI for it)
- Saving and loading (alright, that's probably more than just a minor tweak away, but it'll literally be
  serialising commands)
  
## Which operating systems does it support?

Right now, only Linux is supported, because I haven't had time to get other OSes working. It is, however,
theoretically possible to get SQA working on Windows & OSX - contributors welcome! :P

## How do I build it?

SQA requires a **nightly** version of [Rust](https://www.rust-lang.org/) to run. See [the pertinent Rust Book](https://doc.rust-lang.org/book/nightly-rust.html) section.

SQA uses a few external libraries to do what it does, namely [PortAudio](http://www.portaudio.com/) for
audio output, [GTK+](www.gtk.org) for graphics, and [libMAD](http://www.underbit.com/products/mad/) for
MP3 decoding. If you have Audacity and VLC installed, you probably already have these on your system -
if not, use Google to find instructions on installing these libraries on your Linux distribution of choice.

After you've installed all that, simply clone the repository and run:

    $ cargo run --release

This should compile and run SQA (it might take a while for the first compilation to complete). 
You'll find the output binary in `target/release/sqa`.

## I've got it running, but how the hell do I actually use it?

Yeah, we really need some sort of help file. This explanation will have to do.
First of all, here's a handy graphic detailing some aspects of the UI that I just made in GIMP (forgive me,
it isn't exactly pretty:

-----

**Before you do anything else, hit Ctrl+Enter, then M, then O, then Ctrl+Enter twice.** This initialises the
default audio output, so stuff can actually play back. (If SQA freezes at this point, it's due to PortAudio
being stupid.) Support for choosing audio outputs will come later.

-----

![Help graphic](https://i.imgur.com/g0zAmUh.png)


Basically, everything in SQA is a command - files, stop/start actions, fades - the lot. You can either add
commands to cues (in Blind mode, with the Reorder command) and run them with the cue runner, or go into
Live mode and start firing off commands live.

Commands can be attached to two types of *chains* - either the Unattached chain, used for commands made in
Live (chain **X**) or a numbered cue chain (**Qn**, where n is a number). Note that *the cue runner will
only step through cue numbers incrementally* - for it to work, you need to have Q1 then Q2 then Q3, not
Q1 then Q4 then Q9001. This is a bug and will be fixed.

It's a slightly odd system, I know. I'll include better documentation later. Hopefully this is enough for
you to figure it out - if you're still confused, hit me up on twitter @eeeeeta9 / reddit eeeeeta and I'll
try to explain it for you.

## What license does SQA use?

GPL version 2. Nothing later. This basically means that if you make changes and distribute copies, you need
to send me the source code for your changes. (Read `LICENSE.txt` for more information, along with a
complete copy of the license.)

## How do I generate documentation for the code?

There are some docs, but quite a lot of the codebase is undocumented. Hopefully, you shouldn't find reading
the actual source to be that bad - if you do, hit me up on twitter @eeeeeta9 / reddit eeeeeta
and I'll help you out.

You should be able to use `rustdoc` and Cargo to generate docs:

    $ cargo doc
    $ cargo rustdoc -- --no-defaults --passes "collapse-docs" --passes "unindent-comments"

Check the `target/doc/sqa` folder for output.
