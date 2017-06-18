SQA: the Stuttery QLab Alternative
==================================

*Looking for SQAv1? See the master branch, accessible by faddling about with the "Branch" button somewhere above.*

## wat

This project aims to create an audio player & cue system for live shows and staged productions,
à la Figure53's [QLab](http://figure53.com/qlab/).
All its code is written in the [Rust programming language](https://www.rust-lang.org/), a new language
that prevents memory unsafety and improves programming ergonomics.

This one large repo contains many different crates that all help accomplish that aim. (See the individual crates' README files
for more information!) The crates are distributed in the hope that some of them will be useful outside this project; for example,
`sqa-jack` is a rather nice JACK library.

## why version 2

SQA v1, although it does have a pretty good UI, and was a nice first attempt, isn’t really suitable for its usecase: a **reliable**,
**professional** live theatre audio application with **accurate timing**. It's none of those things in bold. Sticking with the
current codebase is too much effort, when I want to redesign the whole thing - the command system is not ideal, the audio engine
is unsuitable, and that's pretty much the whole application apart from the UI (which may actually be salvaged).

Also, as mentioned above, giving back to the Rust community and all that.

## further information & devlog

Want to follow along with the development of SQA? Check out [pro.theta.eu.org](http://pro.theta.eu.org), where I blog about its
ongoing development ([here's the first post](http://pro.theta.eu.org/2016/12/21/sqa-devlog-0.html)).

## licensing

sqa-backend and sqa-ui are both licensed under the [GNU GPL version 3](https://www.gnu.org/licenses/gpl-3.0.en.html).
Essentially, this means that you have to share your changes if you distribute modified versions of SQA
(as well as enforcing some other freedom-related protections). This is done in the hope that SQA will remain
free and open-source software.

However, the libraries used by and created for SQA, sqa-jack, sqa-ffmpeg, sqa-engine, and sqa-osc-custom-derive,
are licensed under the [Creative Commons Zero](https://creativecommons.org/share-your-work/public-domain/cc0)
license. This means that you can do whatever the hell you want with these libraries.

